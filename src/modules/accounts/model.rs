// Mongo document shape for an account's opening balance, kept apart from the API-facing
// `domain::accounts` so BSON types never leak into payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::accounts::AccountKind;

fn default_version() -> i64 {
    1
}

/// Where an account started. One row per kind, key `aopen_<kind>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountOpeningDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub kind: AccountKind,
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
