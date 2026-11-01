use crate::domain::cash_categories::PayeeType;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// How a payee is stored in the database.
pub struct PayeeDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    /// Only `Vendor` or `Other`: suppliers and staff are not copied here.
    pub payee_type: PayeeType,
    pub name: String,
    pub normalized_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// False once hidden from the pickers; the row stays because history points at its key.
    pub active: bool,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
