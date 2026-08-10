use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceCounterDocument {
    #[serde(rename = "_id")]
    pub name: String,
    #[serde(default)]
    pub next_val: i64,
    pub prefix: String,
    pub padding: i32,
    pub updated_at: BsonDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceBlockDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub name: String,
    pub start: i64,
    pub end: i64,
    pub device_id: Option<String>,
    pub created_at: BsonDateTime,
    pub expires_at: BsonDateTime,
}
