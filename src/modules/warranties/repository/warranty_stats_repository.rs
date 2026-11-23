//! Reads behind the warranty dashboard and report.

use super::warranties_repository::{WARRANTY_COLUMNS, row_to_warranty, warranties};
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::warranties::model::WarrantyDocument;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use sqlx::Row;

/// How many warranties cover their item on a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InForceCounts {
    /// Not held by a claim, and not ended (or never ending).
    pub in_force: u64,
    /// Of those, ending by the given last day.
    pub ending_soon: u64,
}

/// A `$count` facet branch's number; an empty branch means none matched.
fn facet_count(result: &Document, branch: &str) -> AppResult<u64> {
    let rows = result
        .get_array(branch)
        .map_err(|err| AppError::internal(format!("facet branch `{branch}` is missing: {err}")))?;
    let Some(first) = rows.first() else {
        return Ok(0);
    };
    let row = first.as_document().ok_or_else(|| {
        AppError::internal(format!("facet branch `{branch}` row is not a document"))
    })?;
    let count = row
        .get_i32("count")
        .map(i64::from)
        .or_else(|_| row.get_i64("count"))
        .map_err(|err| AppError::internal(format!("facet `{branch}` count is invalid: {err}")))?;
    Ok(u64::try_from(count).expect("a count is never negative"))
}

/// Counts the warranties in force on `today` and those ending by `until` (both `YYYY-MM-DD`),
/// in one round trip.
pub(crate) async fn count_in_force(db: &Db, today: &str, until: &str) -> AppResult<InForceCounts> {
    match db {
        Db::Mongo(mongo) => {
            let in_force = doc! {
                "status": "active",
                "$or": [{ "ends_on": null }, { "ends_on": { "$gte": today } }],
            };
            let ending = doc! { "status": "active", "ends_on": { "$gte": today, "$lte": until } };
            let pipeline = vec![doc! {
                "$facet": {
                    "in_force": [{ "$match": in_force }, { "$count": "count" }],
                    "ending_soon": [{ "$match": ending }, { "$count": "count" }],
                }
            }];
            let mut cursor = warranties(mongo).aggregate(pipeline).await?;
            let result = cursor
                .try_next()
                .await?
                .ok_or_else(|| AppError::internal("a $facet stage always returns one row"))?;
            Ok(InForceCounts {
                in_force: facet_count(&result, "in_force")?,
                ending_soon: facet_count(&result, "ending_soon")?,
            })
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT \
                   COUNT(CASE WHEN ends_on IS NULL OR ends_on >= ? THEN 1 END) AS in_force, \
                   COUNT(CASE WHEN ends_on >= ? AND ends_on <= ? THEN 1 END) AS ending_soon \
                 FROM warranties WHERE status = 'active'",
            )
            .bind(today)
            .bind(today)
            .bind(until)
            .fetch_one(pool)
            .await?;
            let count = |column: &str| {
                u64::try_from(row.get::<i64, _>(column)).expect("a count is never negative")
            };
            Ok(InForceCounts {
                in_force: count("in_force"),
                ending_soon: count("ending_soon"),
            })
        }
    }
}

/// Every warranty given in `[start, end)`.
pub(crate) async fn issued_between(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<WarrantyDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let cursor = warranties(mongo)
                .find(doc! {
                    "created_at": {
                        "$gte": BsonDateTime::from_chrono(start),
                        "$lt": BsonDateTime::from_chrono(end),
                    }
                })
                .await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => sqlx::query(&format!(
            "SELECT {WARRANTY_COLUMNS} FROM warranties WHERE created_at >= ? AND created_at < ?"
        ))
        .bind(start.to_rfc3339())
        .bind(end.to_rfc3339())
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_warranty)
        .collect(),
    }
}
