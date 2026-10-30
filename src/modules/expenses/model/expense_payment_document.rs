use super::default_version::default_version;
use crate::domain::expenses::PaidFrom;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// One payment against an expense.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpensePaymentDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub expense_key: String,
    pub amount_cents: i64,
    pub paid_from: PaidFrom,
    /// `YYYY-MM-DD`.
    pub paid_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
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
