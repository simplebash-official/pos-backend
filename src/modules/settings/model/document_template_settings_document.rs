use super::default_version::default_version;

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The shop's chosen design family per document kind (`kind slug -> family`)
/// and the option values it saved per design (`family -> key -> value`).
/// One row per shop (key `main`); tenant-scoped like `shop_profiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTemplateSettingsDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    #[serde(default)]
    pub selections: HashMap<String, String>,
    #[serde(default)]
    pub options: HashMap<String, HashMap<String, serde_json::Value>>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}
