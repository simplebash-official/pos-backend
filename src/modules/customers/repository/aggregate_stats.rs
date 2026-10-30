use super::customers::customers;
use super::stats_aggregate_result::StatsAggregateResult;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};

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

pub(crate) async fn aggregate_stats(db: &Db) -> AppResult<StatsAggregateResult> {
    match db {
        Db::Mongo(db) => {
            let not_deleted = doc! { "deleted_at": { "$exists": false } };
            let pipeline = vec![doc! {
                "$facet": {
                    "total": [
                        { "$match": not_deleted.clone() },
                        { "$count": "count" }
                    ],
                    "debtors": [
                        { "$match": { "$and": [not_deleted, { "outstanding_balance_cents": { "$gt": 0 } }] } },
                        { "$group": { "_id": null, "sum": { "$sum": "$outstanding_balance_cents" }, "count": { "$sum": 1 } } }
                    ],
                }
            }];

            let mut cursor = customers(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(StatsAggregateResult {
                    total_customers: 0,
                    total_balance_due_cents: 0,
                    active_debtors_count: 0,
                });
            };

            let total_customers = i64_from_facet_branch(&result, "total", "count") as u64;
            let total_balance_due_cents = i64_from_facet_branch(&result, "debtors", "sum");
            let active_debtors_count = i64_from_facet_branch(&result, "debtors", "count") as u64;

            Ok(StatsAggregateResult {
                total_customers,
                total_balance_due_cents,
                active_debtors_count,
            })
        }
        Db::Sqlite(pool) => {
            #[derive(Debug, sqlx::FromRow)]
            struct SqliteStats {
                total_customers: i64,
                total_balance_due_cents: i64,
                active_debtors_count: i64,
            }

            let stats = sqlx::query_as::<_, SqliteStats>(
                r#"
                SELECT
                    COUNT(*) as total_customers,
                    COALESCE(SUM(CASE WHEN outstanding_balance_cents > 0 THEN outstanding_balance_cents ELSE 0 END), 0) as total_balance_due_cents,
                    COUNT(CASE WHEN outstanding_balance_cents > 0 THEN 1 ELSE NULL END) as active_debtors_count
                FROM customers
                WHERE deleted_at IS NULL
                "#,
            )
            .fetch_one(pool)
            .await?;

            Ok(StatsAggregateResult {
                total_customers: stats.total_customers as u64,
                total_balance_due_cents: stats.total_balance_due_cents,
                active_debtors_count: stats.active_debtors_count as u64,
            })
        }
    }
}
