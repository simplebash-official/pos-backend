//! Billing statistics aggregation.

use super::billing_shared::invoices;

use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use sqlx::Row;

/// Extracts a scalar `i64` field pushed by a `$group` facet branch, out of
/// that branch's array-of-one-document shape (`[{ "_id": null, field: N }]`,
/// empty array if the branch matched nothing).
fn i64_from_facet_branch(result: &Document, branch: &str, field: &str) -> AppResult<i64> {
    let rows = result
        .get_array(branch)
        .map_err(|err| AppError::internal(format!("facet branch `{branch}` is missing: {err}")))?;
    // An empty branch means the filter matched nothing: the sum of no rows is zero.
    let Some(first) = rows.first() else {
        return Ok(0);
    };
    let doc = first.as_document().ok_or_else(|| {
        AppError::internal(format!("facet branch `{branch}` row is not a document"))
    })?;
    doc.get_i64(field)
        .or_else(|_| doc.get_i32(field).map(i64::from))
        .map_err(|err| AppError::internal(format!("facet field `{field}` is invalid: {err}")))
}

/// Runs a single `$facet` aggregation over `invoices` to compute the Sales &
/// Invoices History screen's 4 KPI cards in one database round-trip:
/// today's sales sum + count, and the all-time outstanding-credit sum.
pub(crate) async fn aggregate_stats(
    db: &Db,
    today_start: DateTime<Utc>,
    today_end: DateTime<Utc>,
) -> AppResult<StatsAggregateResult> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![doc! {
                "$facet": {
                    "today": [
                        {
                            "$match": {
                                "created_at": {
                                    "$gte": BsonDateTime::from_chrono(today_start),
                                    "$lt": BsonDateTime::from_chrono(today_end),
                                }
                            }
                        },
                        { "$group": { "_id": null, "sum": { "$sum": "$total_cents" }, "count": { "$sum": 1 } } }
                    ],
                    "outstanding": [
                        { "$match": { "status": { "$in": ["pending", "partially_paid"] } } },
                        { "$lookup": {
                            "from": "payments",
                            "localField": "key",
                            "foreignField": "invoice_key",
                            "as": "pmts",
                        } },
                        { "$project": {
                            "balance_due": {
                                "$max": [0, { "$subtract": ["$total_cents", { "$sum": "$pmts.amount_cents" }] }]
                            }
                        } },
                        { "$group": { "_id": null, "sum": { "$sum": "$balance_due" } } }
                    ],
                }
            }];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(StatsAggregateResult {
                    today_sales_cents: 0,
                    today_invoice_count: 0,
                    outstanding_credit_cents: 0,
                });
            };

            let today_sales_cents = i64_from_facet_branch(&result, "today", "sum")?;
            let today_invoice_count = i64_from_facet_branch(&result, "today", "count")? as u64;
            let outstanding_credit_cents = i64_from_facet_branch(&result, "outstanding", "sum")?;

            Ok(StatsAggregateResult {
                today_sales_cents,
                today_invoice_count,
                outstanding_credit_cents,
            })
        }
        Db::Sqlite(pool) => {
            let start_iso = today_start.to_rfc3339();
            let end_iso = today_end.to_rfc3339();

            let row = sqlx::query(
                r#"
                SELECT
                    COALESCE(SUM(CASE WHEN created_at >= ? AND created_at < ? THEN total_cents ELSE 0 END), 0) as today_sales_cents,
                    COUNT(CASE WHEN created_at >= ? AND created_at < ? THEN 1 ELSE NULL END) as today_invoice_count,
                    COALESCE(SUM(CASE WHEN status IN ('pending', 'partially_paid') THEN
                        MAX(0, total_cents - COALESCE((SELECT SUM(amount_cents) FROM payments WHERE payments.invoice_key = invoices.key AND payments.deleted_at IS NULL), 0))
                    ELSE 0 END), 0) as outstanding_credit_cents
                FROM invoices
                WHERE deleted_at IS NULL
                "#,
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_one(pool)
            .await?;

            let today_sales_cents: i64 = row.get("today_sales_cents");
            let today_invoice_count: i64 = row.get("today_invoice_count");
            let outstanding_credit_cents: i64 = row.get("outstanding_credit_cents");

            Ok(StatsAggregateResult {
                today_sales_cents,
                today_invoice_count: today_invoice_count as u64,
                outstanding_credit_cents,
            })
        }
    }
}

/// Everything `service::get_billing_stats` needs from `invoices`, computed in
/// one aggregation round-trip.
#[derive(Debug)]
pub(crate) struct StatsAggregateResult {
    /// Sum of `total_cents` for invoices created in `[today_start, today_end)`.
    pub today_sales_cents: i64,
    /// Count of invoices created in that same window.
    pub today_invoice_count: u64,
    /// Sum of `total_cents` across ALL invoices, unscoped by date, where
    /// `is_credit == true` or `status` is `pending`/`partially_paid`.
    pub outstanding_credit_cents: i64,
}
