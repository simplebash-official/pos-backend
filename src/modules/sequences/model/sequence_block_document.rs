use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape tracking reserved range of sequence numbers for offline/device sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceBlockDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this sequence reservation block.
    pub key: String,
    /// Name of sequence (e.g. "invoice", "repair").
    pub name: String,
    /// Starting integer index of reserved range.
    pub start: i64,
    /// Ending integer index of reserved range.
    pub end: i64,
    /// Optional identifier of device reserving the sequence range.
    pub device_id: Option<String>,
    /// Timestamp when sequence block was reserved.
    pub created_at: BsonDateTime,
    /// Expiration timestamp after which unused block numbers expire.
    pub expires_at: BsonDateTime,
}
