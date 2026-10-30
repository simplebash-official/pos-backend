use super::super::sales::get_i64_flexible;
use super::aging_invoice_row::AgingInvoiceRow;
use super::invoices::invoices;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Every open credit invoice (`pending`/`partially_paid`) with its current
/// balance and age in days relative to `as_of`. The service layer buckets
/// these and derives the top-debtor list.
pub(crate) async fn aggregate_receivables_aging(
    db: &Db,
    as_of: DateTime<Utc>,
) -> AppResult<Vec<AgingInvoiceRow>> {
    match db {
        Db::Mongo(db) => {
            let as_of_bson = BsonDateTime::from_chrono(as_of);
            let pipeline = vec![
                doc! { "$match": { "status": { "$in": ["pending", "partially_paid"] } } },
                doc! {
                    "$lookup": {
                        "from": "payments",
                        "localField": "key",
                        "foreignField": "invoice_key",
                        "as": "pd",
                    }
                },
                doc! {
                    "$project": {
                        "customer_key": 1,
                        "customer_name_snapshot": 1,
                        "customer_phone_snapshot": 1,
                        "due_date": 1,
                        "created_at": 1,
                        "balance_due": {
                            "$max": [0, { "$subtract": ["$total_cents", { "$sum": "$pd.amount_cents" }] }]
                        },
                    }
                },
                doc! { "$match": { "balance_due": { "$gt": 0 } } },
                doc! {
                    "$addFields": {
                        "anchor": {
                            "$dateFromString": {
                                "dateString": { "$ifNull": ["$due_date", null] },
                                "onError": "$created_at",
                                "onNull": "$created_at",
                            }
                        }
                    }
                },
                doc! {
                    "$addFields": {
                        "days": { "$dateDiff": { "startDate": "$anchor", "endDate": as_of_bson, "unit": "day" } }
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                out.push(AgingInvoiceRow {
                    customer_key: row.get_str("customer_key").ok().map(String::from),
                    customer_name: row
                        .get_str("customer_name_snapshot")
                        .unwrap_or("Unknown")
                        .to_string(),
                    customer_phone: row
                        .get_str("customer_phone_snapshot")
                        .ok()
                        .map(String::from),
                    balance_due_cents: get_i64_flexible(&row, "balance_due"),
                    days_overdue: get_i64_flexible(&row, "days") as i32,
                });
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let rows = sqlx::query(
                "SELECT i.customer_key, i.customer_name_snapshot, i.customer_phone_snapshot, i.due_date, i.created_at, i.total_cents, COALESCE(SUM(p.amount_cents), 0) as paid_cents FROM invoices i LEFT JOIN payments p ON p.invoice_key = i.key WHERE i.status IN ('pending', 'partially_paid') GROUP BY i.key"
            )
            .fetch_all(pool)
            .await?;

            let mut out = Vec::new();
            for r in rows {
                let total_cents: i64 = r.get("total_cents");
                let paid_cents: i64 = r.get("paid_cents");
                let balance = (total_cents - paid_cents).max(0);
                if balance <= 0 {
                    continue;
                }

                let due_date_opt: Option<String> = r.get("due_date");
                let created_at_str: String = r.get("created_at");

                let anchor = due_date_opt
                    .as_deref()
                    .and_then(|d| {
                        DateTime::parse_from_rfc3339(d)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
                    .or_else(|| {
                        DateTime::parse_from_rfc3339(&created_at_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
                    .unwrap_or(as_of);

                let days = (as_of - anchor).num_days() as i32;

                out.push(AgingInvoiceRow {
                    customer_key: r.get("customer_key"),
                    customer_name: r
                        .try_get("customer_name_snapshot")
                        .unwrap_or_else(|_| "Unknown".to_string()),
                    customer_phone: r.get("customer_phone_snapshot"),
                    balance_due_cents: balance,
                    days_overdue: days,
                });
            }
            Ok(out)
        }
    }
}
