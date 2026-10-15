// Storage document shape for a shop's own label designs. Kept apart from
// `domain::labels`, which holds the API-facing types.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::labels::{LabelElement, LabelTemplate};

/// A label design a shop built (`label_templates`). Tenant-scoped like every
/// other per-shop collection; not a synced resource (shop settings are not).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelTemplateDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<LabelElement>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

fn default_version() -> i64 {
    1
}

impl LabelTemplateDocument {
    pub fn into_template(self) -> LabelTemplate {
        LabelTemplate {
            key: self.key,
            name: self.name,
            width_mm: self.width_mm,
            height_mm: self.height_mm,
            elements: self.elements,
            built_in: false,
            created_at: Some(self.created_at.to_chrono()),
            updated_at: Some(self.updated_at.to_chrono()),
        }
    }
}
