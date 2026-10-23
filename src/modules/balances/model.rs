// Mongo document shapes for balance openings and advance recoveries, kept apart from the
// API-facing `domain::balances` so BSON types never leak into payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::balances::BalanceSubject;

fn default_version() -> i64 {
    1
}

/// Where a supplier balance (or a staff member's earlier advance) starts. Key `bopen_<subject key>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceOpeningDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub subject_kind: BalanceSubject,
    pub subject_key: String,
    pub opening_balance_cents: i64,
    /// `YYYY-MM-DD`.
    pub as_of_date: String,
    pub set_by_name_snapshot: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Part of a staff advance taken back from pay. Immutable once saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceRecoveryDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub employee_key: String,
    pub amount_cents: i64,
    /// `YYYY-MM-DD`.
    pub recovered_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
