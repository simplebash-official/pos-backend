use super::dto::dto;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::attachments::{Attachment, AttachmentOwner};
use crate::modules::attachments::repository;

/// The photos of one record (details only).
pub async fn list_for(
    db: &Db,
    kind: AttachmentOwner,
    owner_key: &str,
) -> AppResult<Vec<Attachment>> {
    Ok(repository::list_for_owner(db, kind, owner_key)
        .await?
        .iter()
        .map(dto)
        .collect())
}
