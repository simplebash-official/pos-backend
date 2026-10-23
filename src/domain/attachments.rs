// Pure business types for receipt photos attached to a record. No I/O, no framework types beyond
// serde/utoipa derives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// What a photo is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentOwner {
    CashMovement,
    Expense,
    Income,
}

impl AttachmentOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentOwner::CashMovement => "cash_movement",
            AttachmentOwner::Expense => "expense",
            AttachmentOwner::Income => "income",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cash_movement" => Some(AttachmentOwner::CashMovement),
            "expense" => Some(AttachmentOwner::Expense),
            "income" => Some(AttachmentOwner::Income),
            _ => None,
        }
    }
}

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

/// Query for `GET /attachments`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AttachmentListQuery {
    pub owner_kind: AttachmentOwner,
    pub owner_key: String,
}
