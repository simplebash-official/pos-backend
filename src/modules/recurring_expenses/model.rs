// Mongo document shape for regular bills, kept apart from the API-facing
// `domain::recurring_expenses` so BSON types never leak into payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::expenses::PaidFrom;

fn default_version() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringExpenseDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub category_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    pub amount_cents: i64,
    pub due_day: i64,
    pub remind_days_before: i64,
    pub paid_from: PaidFrom,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub active: bool,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
