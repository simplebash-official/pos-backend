use super::super::sales::get_i64_flexible;
use super::align_bucket_instant::align_bucket_instant;
use super::trunc::trunc;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;
use crate::domain::reports::Granularity;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

/// `Σ earned commission` per `$dateTrunc` bucket, across delivered/completed
/// repairs + print jobs in `[start, end)`. Bucketed on the job's `created_at`
/// to match how revenue is recognised on the invoice date.
pub(crate) async fn aggregate_commission_series(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    granularity: Granularity,
) -> AppResult<std::collections::BTreeMap<i64, i64>> {
    match db {
        Db::Mongo(db) => {
            use crate::core::calculations::commission_cents;

            let unit = granularity.as_mongo_unit();
            let mut out: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();

            for collection in ["repairs", "print_jobs"] {
                let pipeline = vec![
                    doc! {
                        "$match": {
                            "deleted_at": null,
                            "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
                            "created_at": {
                                "$gte": BsonDateTime::from_chrono(start),
                                "$lt": BsonDateTime::from_chrono(end),
                            }
                        }
                    },
                    doc! {
                        "$project": {
                            "bucket": trunc(unit),
                            "estimated_cost_cents": 1,
                            "material_cost_cents": 1,
                            "split_type": 1,
                            "split_value": 1,
                        }
                    },
                ];

                let mut cursor = db
                    .collection::<Document>(collection)
                    .aggregate(pipeline)
                    .await?;
                while let Some(row) = cursor.try_next().await? {
                    let Some(ts) = row.get_datetime("bucket").ok().map(|d| d.to_chrono()) else {
                        continue;
                    };
                    let revenue = get_i64_flexible(&row, "estimated_cost_cents");
                    let material = get_i64_flexible(&row, "material_cost_cents");
                    let profit = crate::core::calculations::job_profit_cents(revenue, material);
                    let split_type = row.get_str("split_type").ok();
                    let split_value = row
                        .get_f64("split_value")
                        .ok()
                        .or_else(|| row.get_i64("split_value").ok().map(|v| v as f64))
                        .or_else(|| row.get_i32("split_value").ok().map(|v| v as f64))
                        .unwrap_or(0.0);
                    let earned = commission_cents(profit, split_type, split_value);
                    *out.entry(ts.timestamp_millis()).or_insert(0) += earned;
                }
            }

            Ok(out)
        }
        Db::Sqlite(pool) => {
            use crate::core::calculations::commission_cents;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let mut out: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();

            for table in ["repairs", "print_jobs"] {
                let query_str = format!(
                    "SELECT created_at, estimated_cost_cents, material_cost_cents, split_type, split_value FROM {table} WHERE deleted_at IS NULL AND status IN ('completed', 'delivered', 'ready_for_pickup') AND created_at >= ? AND created_at < ?"
                );
                let rows = sqlx::query(&query_str)
                    .bind(&start_iso)
                    .bind(&end_iso)
                    .fetch_all(pool)
                    .await?;

                for row in &rows {
                    let doc = map_sqlite_row_to_document(row);
                    let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                        bdt.to_chrono()
                    } else {
                        continue;
                    };
                    let bucket_start = align_bucket_instant(created_at, granularity);
                    let revenue = get_i64_flexible(&doc, "estimated_cost_cents");
                    let material = get_i64_flexible(&doc, "material_cost_cents");
                    let profit = crate::core::calculations::job_profit_cents(revenue, material);
                    let split_type = doc.get_str("split_type").ok();
                    let split_value = doc
                        .get_f64("split_value")
                        .ok()
                        .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
                        .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
                        .unwrap_or(0.0);
                    let earned = commission_cents(profit, split_type, split_value);
                    *out.entry(bucket_start.timestamp_millis()).or_insert(0) += earned;
                }
            }

            Ok(out)
        }
    }
}
