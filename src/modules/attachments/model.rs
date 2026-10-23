// Mongo document shape for a receipt photo, kept apart from the API-facing `domain::attachments`
// so BSON types never leak into payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::attachments::AttachmentOwner;

fn default_version() -> i64 {
    1
}

/// A receipt photo. Immutable once saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub owner_kind: AttachmentOwner,
    pub owner_key: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub data_base64: String,
    pub uploaded_by_user_id: String,
    pub uploaded_by_name_snapshot: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
