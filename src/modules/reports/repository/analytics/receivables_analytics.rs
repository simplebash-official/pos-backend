//! Receivables aging and customer balances.

use super::super::sales::get_i64_flexible;
use super::analytics_shared::invoices;

use crate::clients::db::Db;

use crate::core::error::AppResult;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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
                    balance_due_cents: get_i64_flexible(&row, "balance_due")?,
                    days_overdue: get_i64_flexible(&row, "days")? as i32,
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

// ---------------------------------------------------------------------------
// receivables aging
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct AgingInvoiceRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub balance_due_cents: i64,
    pub days_overdue: i32,
}

/// `customer.key -> outstanding_balance_cents` for the given keys.
pub(crate) async fn fetch_customer_balances(
    db: &Db,
    keys: &[String],
) -> AppResult<std::collections::HashMap<String, i64>> {
    match db {
        Db::Mongo(db) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            let mut cursor = db
                .collection::<Document>("customers")
                .find(doc! { "key": { "$in": keys } })
                .projection(doc! { "key": 1, "outstanding_balance_cents": 1 })
                .await?;
            while let Some(row) = cursor.try_next().await? {
                if let Ok(k) = row.get_str("key") {
                    out.insert(
                        k.to_string(),
                        get_i64_flexible(&row, "outstanding_balance_cents")?,
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            use sqlx::Row;
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT key, outstanding_balance_cents FROM customers WHERE key IN ({placeholders})"
            );
            let mut q = sqlx::query(&sql);
            for k in keys {
                q = q.bind(k);
            }
            let rows = q.fetch_all(pool).await?;
            for r in rows {
                let k: String = r.get("key");
                let bal: i64 = r.try_get("outstanding_balance_cents").unwrap_or(0);
                out.insert(k, bal);
            }
            Ok(out)
        }
    }
}
