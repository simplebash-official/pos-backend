use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// One user has read one notification. `created_at` is when they read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationReadDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// `nrd_<hash of user + notification>`, the same on every replica.
    pub key: String,
    pub notification_key: String,
    pub user_key: String,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
