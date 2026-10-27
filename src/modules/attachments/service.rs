// Business rules for receipt photos: what can be attached, to what, and how many. The image is
// checked by its bytes, not by the name or type the client claims.

use base64::{Engine, engine::general_purpose::STANDARD};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        logging::domain::tracked,
    },
    domain::attachments::{Attachment, AttachmentOwner, UploadAttachmentRequest},
    modules::{
        attachments::{model::AttachmentDocument, repository},
        expenses, incomes, sessions,
    },
};

/// Largest image once decoded. The app shrinks a photo well below this before sending it.
pub const MAX_BYTES: usize = 1_048_576;
/// Photos one record can hold (a receipt and a couple of pages).
const MAX_PER_RECORD: usize = 5;
const NAME_MAX: usize = 120;

/// The image type the bytes really are, from their first bytes; `None` for anything else.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn dto(d: &AttachmentDocument) -> Attachment {
    Attachment {
        key: d.key.clone(),
        owner_kind: d.owner_kind,
        owner_key: d.owner_key.clone(),
        file_name: d.file_name.clone(),
        content_type: d.content_type.clone(),
        size_bytes: d.size_bytes,
        uploaded_by_name_snapshot: d.uploaded_by_name_snapshot.clone(),
        created_at: d.created_at.to_chrono(),
    }
}

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

/// One photo with its image bytes.
pub async fn get_content(db: &Db, key: &str) -> AppResult<(AttachmentDocument, Vec<u8>)> {
    let doc = repository::find_by_key(db, key).await?.ok_or_else(|| {
        AppError::not_found_with_code("Photo not found.", codes::ATTACHMENT_NOT_FOUND)
    })?;
    let bytes = STANDARD
        .decode(&doc.data_base64)
        .map_err(|_| AppError::internal("A stored photo could not be decoded".to_string()))?;
    Ok((doc, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_are_recognised_by_their_bytes() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]), Some("image/jpeg"));
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(sniff(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"%PDF-1.4"), None);
        assert_eq!(sniff(b"<svg></svg>"), None);
    }
}
