use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::attachments::AttachmentOwner;
use crate::modules::attachments::model::AttachmentDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<AttachmentDocument> {
    let kind: String = row.get("owner_kind");
    Ok(AttachmentDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        owner_kind: AttachmentOwner::parse(&kind).ok_or_else(|| {
            AppError::internal(format!("Unknown attachment owner '{kind}' in database"))
        })?,
        owner_key: row.get("owner_key"),
        file_name: row.get("file_name"),
        content_type: row.get("content_type"),
        size_bytes: row.get("size_bytes"),
        data_base64: row.get("data_base64"),
        uploaded_by_user_id: row.get("uploaded_by_user_id"),
        uploaded_by_name_snapshot: row.get("uploaded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}
