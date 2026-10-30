use super::attachments::attachments;
use super::from_row::from_row;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::attachments::model::AttachmentDocument;
use mongodb::bson::doc;

/// One photo by key, image included.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<AttachmentDocument>> {
    match db {
        Db::Mongo(db) => Ok(attachments(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM attachments WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}
