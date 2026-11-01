use crate::domain::balances::BalanceSubject;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Part of a staff advance taken back from pay. Immutable once saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceRecoveryDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub employee_key: String,
    pub amount_cents: i64,
    /// `YYYY-MM-DD`.
    pub recovered_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Where a supplier balance (or a staff member's earlier advance) starts. Key `bopen_<subject key>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceOpeningDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub subject_kind: BalanceSubject,
    pub subject_key: String,
    pub opening_balance_cents: i64,
    /// `YYYY-MM-DD`.
    pub as_of_date: String,
    pub set_by_name_snapshot: String,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
