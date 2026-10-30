use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

fn default_version() -> i64 {
    1
}

/// The one drawer-rules row, key `cset_shop`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashSettingsDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub undo_window_seconds: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_threshold_cents: Option<i64>,
    pub quick_amounts_cents: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float_target_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_name_snapshot: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
