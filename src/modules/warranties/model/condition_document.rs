//! How shop warranty condition templates are stored.

use crate::domain::warranties::WarrantyCondition;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// How a warranty condition template is stored in `warranty_condition_templates`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarrantyConditionDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// `wcon_…`
    pub key: String,
    /// Human description (e.g. "Physical damage / cracks").
    pub name: String,
    /// Whether this condition is active and offered for new claims.
    pub active: bool,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
}

impl WarrantyConditionDocument {
    #[must_use]
    pub fn into_condition(self) -> WarrantyCondition {
        WarrantyCondition {
            key: self.key,
            name: self.name,
            active: self.active,
            created_at: self.created_at.to_chrono(),
        }
    }
}
