use super::attachments::attachments;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::attachments::model::AttachmentDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a photo.
pub(crate) async fn insert(
    db: &Db,
    mut document: AttachmentDocument,
) -> AppResult<AttachmentDocument> {
    match db {
        Db::Mongo(db) => {
            let res = attachments(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO attachments (
                    key, id, owner_kind, owner_key, file_name, content_type, size_bytes, data_base64,
                    uploaded_by_user_id, uploaded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.owner_kind.as_str())
            .bind(&document.owner_key)
            .bind(&document.file_name)
            .bind(&document.content_type)
            .bind(document.size_bytes)
            .bind(&document.data_base64)
            .bind(&document.uploaded_by_user_id)
            .bind(&document.uploaded_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(pool)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
