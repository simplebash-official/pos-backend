use super::attachment_owner::AttachmentOwner;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// A receipt photo's details. The image itself is read from `GET /attachments/{key}/content`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub key: String,
    pub owner_kind: AttachmentOwner,
    pub owner_key: String,
    pub file_name: String,
    /// `image/jpeg`, `image/png` or `image/webp`.
    pub content_type: String,
    pub size_bytes: i64,
    pub uploaded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}
