use super::print_jobs::print_jobs;
use super::stats_aggregate_result::StatsAggregateResult;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

use sqlx::Row;

/// Extracts a scalar `i64` field pushed by a `$group` facet branch, out of
/// that branch's array-of-one-document shape (`[{ "_id": null, field: N }]`,
/// empty array if the branch matched nothing).
fn i64_from_facet_branch(result: &Document, branch: &str, field: &str) -> i64 {
    result
        .get_array(branch)
        .ok()
        .and_then(|arr| arr.first())
        .and_then(|val| val.as_document())
        .and_then(|doc| {
            doc.get_i64(field)
                .ok()
                .or_else(|| doc.get_i32(field).ok().map(i64::from))
        })
        .unwrap_or(0)
}

/// Runs a single `$facet` aggregation over `print_jobs` to compute the Print
/// Jobs screen's KPI cards in one database round-trip: today's ticket count
/// + revenue sum, and the all-time open-ticket count.
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
                                "deleted_at": { "$exists": false },
                                "created_at": {
                                    "$gte": BsonDateTime::from_chrono(today_start),
                                    "$lt": BsonDateTime::from_chrono(today_end),
                                }
                            }
                        },
                        {
                            "$group": {
                                "_id": null,
                                "sum": { "$sum": "$estimated_cost_cents" },
                                "count": { "$sum": 1 },
                            }
                        }
                    ],
                    "pending": [
                        {
                            "$match": {
                                "deleted_at": { "$exists": false },
                                "status": { "$nin": ["delivered", "cancelled"] },
                            }
                        },
                        { "$count": "count" }
                    ],
                }
            }];

            let mut cursor = print_jobs(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(StatsAggregateResult {
                    today_job_count: 0,
                    today_revenue_cents: 0,
                    pending_job_count: 0,
                });
            };

            let today_job_count = i64_from_facet_branch(&result, "today", "count") as u64;
            let today_revenue_cents = i64_from_facet_branch(&result, "today", "sum");
            let pending_job_count = i64_from_facet_branch(&result, "pending", "count") as u64;

            Ok(StatsAggregateResult {
                today_job_count,
                today_revenue_cents,
                pending_job_count,
            })
        }
        Db::Sqlite(pool) => {
            let start_iso = today_start.to_rfc3339();
            let end_iso = today_end.to_rfc3339();

            let row = sqlx::query(
                r#"
                SELECT
                    COUNT(CASE WHEN created_at >= ? AND created_at < ? THEN 1 ELSE NULL END) as today_job_count,
                    COALESCE(SUM(CASE WHEN created_at >= ? AND created_at < ? THEN estimated_cost_cents ELSE 0 END), 0) as today_revenue_cents,
                    COUNT(CASE WHEN status NOT IN ('delivered', 'cancelled') THEN 1 ELSE NULL END) as pending_job_count
                FROM print_jobs
                WHERE deleted_at IS NULL
                "#,
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_one(pool)
            .await?;

            let today_job_count: i64 = row.get("today_job_count");
            let today_revenue_cents: i64 = row.get("today_revenue_cents");
            let pending_job_count: i64 = row.get("pending_job_count");

            Ok(StatsAggregateResult {
                today_job_count: today_job_count as u64,
                today_revenue_cents,
                pending_job_count: pending_job_count as u64,
            })
        }
    }
}
