use super::invoices::invoices;
use super::outstanding_stats_aggregate::OutstandingStatsAggregate;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use chrono::Utc;
use futures_util::TryStreamExt;

use mongodb::bson::doc;
use sqlx::Row;

/// Computes all-time outstanding credit and overdue aging metrics.
pub(crate) async fn aggregate_outstanding_stats(db: &Db) -> AppResult<OutstandingStatsAggregate> {
    let today_str = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let mut agg = OutstandingStatsAggregate::default();

    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! {
                    "$match": {
                        "status": { "$in": ["pending", "partially_paid"] }
                    }
                },
                doc! {
                    "$lookup": {
                        "from": "payments",
                        "localField": "key",
                        "foreignField": "invoice_key",
                        "as": "payments_docs"
                    }
                },
                doc! {
                    "$project": {
                        "total_cents": 1,
                        "due_date": 1,
                        "paid_cents": { "$sum": "$payments_docs.amount_cents" },
                    }
                },
                doc! {
                    "$project": {
                        "total_cents": 1,
                        "due_date": 1,
                        "balance_due": {
                            "$max": [
                                0,
                                { "$subtract": ["$total_cents", "$paid_cents"] }
                            ]
                        }
                    }
                },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "total_balance": { "$sum": "$balance_due" },
                                }
                            }
                        ],
                        "overdue": [
                            {
                                "$match": {
                                    "due_date": { "$ne": null, "$lt": &today_str },
                                    "balance_due": { "$gt": 0 }
                                }
                            },
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "overdue_balance": { "$sum": "$balance_due" },
                                }
                            }
                        ]
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    agg.total_credit_invoices_count =
                        first.get_i32("count").map(|c| c as u64).unwrap_or(0);
                    agg.total_outstanding_cents = first.get_i64("total_balance").unwrap_or(0);
                }

                if let Ok(overdue) = result.get_array("overdue")
                    && let Some(first) = overdue.first().and_then(|v| v.as_document())
                {
                    agg.overdue_invoices_count =
                        first.get_i32("count").map(|c| c as u64).unwrap_or(0);
                    agg.overdue_amount_cents = first.get_i64("overdue_balance").unwrap_or(0);
                }
            }
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"SELECT
                    COALESCE(SUM(i.total_cents - COALESCE(p.paid_cents, 0)), 0) as total_balance,
                    COUNT(*) as count,
                    COALESCE(SUM(CASE WHEN i.due_date IS NOT NULL AND i.due_date < ? AND (i.total_cents - COALESCE(p.paid_cents, 0)) > 0 THEN 1 ELSE 0 END), 0) as overdue_count,
                    COALESCE(SUM(CASE WHEN i.due_date IS NOT NULL AND i.due_date < ? AND (i.total_cents - COALESCE(p.paid_cents, 0)) > 0 THEN (i.total_cents - COALESCE(p.paid_cents, 0)) ELSE 0 END), 0) as overdue_balance
                FROM invoices i
                LEFT JOIN (
                    SELECT invoice_key, SUM(amount_cents) as paid_cents FROM payments GROUP BY invoice_key
                ) p ON i.key = p.invoice_key
                WHERE i.status IN ('pending', 'partially_paid')"#,
            )
            .bind(&today_str)
            .bind(&today_str)
            .fetch_one(pool)
            .await?;

            let count: i64 = row.get("count");
            let overdue_count: i64 = row.get("overdue_count");
            agg.total_credit_invoices_count = count as u64;
            agg.total_outstanding_cents = row.get("total_balance");
            agg.overdue_invoices_count = overdue_count as u64;
            agg.overdue_amount_cents = row.get("overdue_balance");
        }
    }

    Ok(agg)
}
