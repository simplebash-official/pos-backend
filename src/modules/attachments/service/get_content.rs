use super::attachment_content::AttachmentContent;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::modules::attachments::repository;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// One photo with its image bytes.
pub async fn get_content(db: &Db, key: &str) -> AppResult<AttachmentContent> {
    let doc = repository::find_by_key(db, key).await?.ok_or_else(|| {
        AppError::not_found_with_code("Photo not found.", codes::ATTACHMENT_NOT_FOUND)
    })?;
    let bytes = STANDARD
        .decode(&doc.data_base64)
        .map_err(|_| AppError::internal("A stored photo could not be decoded".to_string()))?;
    Ok(AttachmentContent {
        document: doc,
        bytes,
    })
}
