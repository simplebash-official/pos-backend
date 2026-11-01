use crate::domain::accounts::AccountKind;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Where an account started. One row per kind, key `aopen_<kind>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountOpeningDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub kind: AccountKind,
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
