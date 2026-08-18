use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

/// Mongo document shape tracking auto-increment counter state for an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceCounterDocument {
    /// Sequence counter name/identifier (e.g. "invoice", "repair").
    #[serde(rename = "_id")]
    pub name: String,
    /// Next integer value to allocate for this sequence.
    #[serde(default)]
    pub next_val: i64,
    /// Prefix string attached to formatted sequence numbers (e.g. "INV-").
    pub prefix: String,
    /// Minimum digit padding length for numbers (e.g. 6 digits -> "000001").
    pub padding: i32,
    /// Timestamp when counter was last incremented.
    pub updated_at: BsonDateTime,
}

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
