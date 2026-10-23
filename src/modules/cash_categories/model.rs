// Mongo document shape for drawer cash categories, kept apart from the
// API-facing `domain::cash_categories` so BSON types never leak into payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::{
    cash_categories::{CashCategory, ReportGroup, ReviewStatus},
    sessions::CashMovementType,
};

fn default_version() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashCategoryDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub movement_type: CashMovementType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_si: Option<String>,
    /// Lowercase, whitespace-free form used for the duplicate check and search.
    pub normalized_name: String,
    pub report_group: ReportGroup,
    pub is_system: bool,
    pub review_status: ReviewStatus,
    pub pinned: bool,
    pub sort_order: i64,
    pub active: bool,
    pub requires_receipt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_approval_over_cents: Option<i64>,
    /// Set when this category was merged away: readers follow it to the target.
    /// Movements keep pointing at the old key (they are append-only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_into_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_name_snapshot: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl CashCategoryDocument {
    /// `usage_count` is counted at read time from the movements, not stored.
    pub fn into_cash_category(self, usage_count: i64) -> CashCategory {
        CashCategory {
            id: self
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(|| self.key.clone()),
            key: self.key,
            movement_type: self.movement_type,
            name: self.name,
            name_si: self.name_si,
            report_group: self.report_group,
            is_system: self.is_system,
            review_status: self.review_status,
            pinned: self.pinned,
            sort_order: self.sort_order,
            active: self.active,
            requires_receipt: self.requires_receipt,
            requires_approval_over_cents: self.requires_approval_over_cents,
            usage_count,
            created_by_name_snapshot: self.created_by_name_snapshot,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
