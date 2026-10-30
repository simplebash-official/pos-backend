use mongodb::bson::DateTime as BsonDateTime;
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
