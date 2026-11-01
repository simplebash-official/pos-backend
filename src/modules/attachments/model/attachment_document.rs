use crate::domain::attachments::AttachmentOwner;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A receipt photo. Immutable once saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub owner_kind: AttachmentOwner,
    pub owner_key: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub data_base64: String,
    pub uploaded_by_user_id: String,
    pub uploaded_by_name_snapshot: String,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
