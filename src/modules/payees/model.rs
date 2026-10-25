// Mongo document shape for stored payees (vendors and free-typed payees).

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::cash_categories::PayeeType;

fn default_version() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayeeDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    /// Only `Vendor` or `Other`: suppliers and staff are not copied here.
    pub payee_type: PayeeType,
    pub name: String,
    pub normalized_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// False once hidden from the pickers; the row stays because history points at its key.
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
