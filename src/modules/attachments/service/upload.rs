use super::dto::dto;
use super::max_bytes::MAX_BYTES;
use super::sniff::sniff;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::logging::domain::tracked;
use crate::domain::attachments::{Attachment, AttachmentOwner, UploadAttachmentRequest};
use crate::modules::attachments::model::AttachmentDocument;
use crate::modules::attachments::repository;
use crate::modules::{expenses, incomes, sessions};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mongodb::bson::DateTime as BsonDateTime;

/// Photos one record can hold (a receipt and a couple of pages).
const MAX_PER_RECORD: usize = 5;

const NAME_MAX: usize = 120;

/// Fails with 404 unless the record the photo is for exists.
async fn require_owner(db: &Db, kind: AttachmentOwner, key: &str) -> AppResult<()> {
    match kind {
        AttachmentOwner::CashMovement => {
            sessions::service::get_movement(db, key).await?;
        }
        AttachmentOwner::Expense => {
            expenses::service::get_expense(db, key).await?;
        }
        AttachmentOwner::Income => {
            incomes::service::get_income(db, key).await?;
        }
    }
    Ok(())
}

/// Attaches a photo to a record.
pub async fn upload(
    db: &Db,
    uploaded_by_user_id: String,
    uploaded_by_name: String,
    body: UploadAttachmentRequest,
) -> AppResult<Attachment> {
    tracked("attachments.uploaded", async move {
        let file_name = body.file_name.trim().to_string();
        if file_name.is_empty() || file_name.chars().count() > NAME_MAX {
            return Err(AppError::validation(format!(
                "The file name needs 1 to {NAME_MAX} letters."
            )));
        }
        let bytes = STANDARD
            .decode(body.data_base64.trim())
            .map_err(|_| AppError::validation("The photo could not be read."))?;
        if bytes.is_empty() {
            return Err(AppError::validation("The photo is empty."));
        }
        if bytes.len() > MAX_BYTES {
            return Err(AppError::validation(
                "The photo is too big. Please use one under 1 MB.",
            ));
        }
        let real_type = sniff(&bytes)
            .ok_or_else(|| AppError::validation("Please attach a JPEG, PNG or WebP photo."))?;
        if real_type != body.content_type {
            return Err(AppError::validation(
                "The photo is not the kind of file it says it is.",
            ));
        }
        require_owner(db, body.owner_kind, &body.owner_key).await?;
        if repository::list_for_owner(db, body.owner_kind, &body.owner_key)
            .await?
            .len()
            >= MAX_PER_RECORD
        {
            return Err(AppError::conflict(
                codes::ATTACHMENT_LIMIT,
                format!("A record can hold up to {MAX_PER_RECORD} photos."),
            ));
        }
        let now = BsonDateTime::now();
        let saved = repository::insert(
            db,
            AttachmentDocument {
                id: None,
                key: generate_id(prefixes::ATTACHMENT),
                owner_kind: body.owner_kind,
                owner_key: body.owner_key,
                file_name,
                content_type: real_type.to_string(),
                size_bytes: bytes.len() as i64,
                data_base64: STANDARD.encode(&bytes),
                uploaded_by_user_id,
                uploaded_by_name_snapshot: uploaded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        Ok(dto(&saved))
    })
    .await
}
