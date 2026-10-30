use super::find_by_key::find_by_key;
use super::print_jobs::print_jobs;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;

use crate::core::error::AppResult;
use crate::modules::print_jobs::model::PrintJobDocument;

use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

/// Narrow status-only mutator, looked up by `key` — used by
/// `service::mark_delivered`, the cross-module hook `billing`'s
/// complete-sale flow calls.
pub(crate) async fn set_status_by_key(
    db: &Db,
    key: &str,
    status: &str,
) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one_and_update(
                doc! { "key": key, "deleted_at": { "$exists": false } },
                doc! {
                    "$set": { "status": status, "updated_at": BsonDateTime::now() },
                    "$inc": { "version": 1 },
                },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let now_iso = now_utc_iso();
            let res = sqlx::query(
                "UPDATE print_jobs SET status = ?, version = version + 1, updated_at = ? WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(status)
            .bind(&now_iso)
            .bind(key)
            .execute(pool)
            .await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_by_key(db, key).await
        }
    }
}
