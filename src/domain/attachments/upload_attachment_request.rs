use super::attachment_owner::AttachmentOwner;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /attachments`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadAttachmentRequest {
    pub owner_kind: AttachmentOwner,
    pub owner_key: String,
    pub file_name: String,
    pub content_type: String,
    /// The image bytes, base64 encoded; at most 1 MB once decoded.
    pub data_base64: String,
}
