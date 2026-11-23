//! Reads behind the warranty dashboard and report: rework jobs in a period and the jobs a set of
//! warranties came from. Nothing here writes.

use super::repairs_repository::{repairs, row_to_repair_doc};
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};

const KEYS_PER_QUERY: usize = 500;

/// Rework jobs (free warranty work) opened in `[start, end)`, leaving out cancelled and deleted
/// ones.
pub(crate) async fn rework_jobs_between(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<RepairDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let cursor = repairs(mongo)
                .find(doc! {
                    "kind.kind": "warranty_rework",
                    "status": { "$ne": "cancelled" },
                    "deleted_at": { "$exists": false },
                    "created_at": {
                        "$gte": BsonDateTime::from_chrono(start),
                        "$lt": BsonDateTime::from_chrono(end),
                    },
                })
                .await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => sqlx::query(
            "SELECT * FROM repairs WHERE json_extract(kind, '$.kind') = 'warranty_rework' \
             AND status != 'cancelled' AND deleted_at IS NULL \
             AND created_at >= ? AND created_at < ?",
        )
        .bind(start.to_rfc3339())
        .bind(end.to_rfc3339())
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_repair_doc)
        .collect(),
    }
}

/// The jobs with these keys, deleted ones included (a warranty outlives its job's deletion).
pub(crate) async fn find_by_keys(db: &Db, keys: &[String]) -> AppResult<Vec<RepairDocument>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(mongo) => {
            let cursor = repairs(mongo).find(doc! { "key": { "$in": keys } }).await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => {
            let mut found = Vec::with_capacity(keys.len());
            // Stays well under SQLite's bound-parameter limit.
            for chunk in keys.chunks(KEYS_PER_QUERY) {
                let sql = format!(
                    "SELECT * FROM repairs WHERE key IN ({})",
                    vec!["?"; chunk.len()].join(", ")
                );
                let mut query = sqlx::query(&sql);
                for key in chunk {
                    query = query.bind(key);
                }
                for row in query.fetch_all(pool).await? {
                    found.push(row_to_repair_doc(&row)?);
                }
            }
            Ok(found)
        }
    }
}
