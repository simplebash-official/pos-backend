use super::find_by_key::find_by_key;
use super::repairs::repairs;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

/// Narrow status-only mutator, looked up by `key` — used by
/// `service::mark_delivered`, the cross-module hook `billing`'s
/// complete-sale flow calls. Bypasses the general `update` set-doc builder
/// since this is the one place a ticket's status changes without a client
/// PATCH body.
pub(crate) async fn set_status_by_key(
    db: &Db,
    key: &str,
    status: &str,
) -> AppResult<Option<RepairDocument>> {
    match db {
        Db::Mongo(db) => Ok(repairs(db)
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
                "UPDATE repairs SET status = ?, version = version + 1, updated_at = ? WHERE key = ? AND deleted_at IS NULL",
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
