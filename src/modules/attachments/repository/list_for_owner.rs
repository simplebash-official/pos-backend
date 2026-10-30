use super::attachments::attachments;
use super::from_row::from_row;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::attachments::AttachmentOwner;
use crate::modules::attachments::model::AttachmentDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// The photos of one record, oldest first.
pub(crate) async fn list_for_owner(
    db: &Db,
    owner_kind: AttachmentOwner,
    owner_key: &str,
) -> AppResult<Vec<AttachmentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = attachments(db)
                .find(doc! { "owner_kind": owner_kind.as_str(), "owner_key": owner_key })
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM attachments WHERE owner_kind = ? AND owner_key = ? ORDER BY created_at ASC",
            )
            .bind(owner_kind.as_str())
            .bind(owner_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(from_row).collect()
        }
    }
}
