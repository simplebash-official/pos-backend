use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document};

/// A payload split into what the apply step needs.
#[derive(Debug)]
pub(crate) struct Encoded {
    /// Legacy `_id`, preserved from the payload's `id`.
    pub id: ObjectId,
    /// Ordinary (non-derived) stored fields, snake_case, typed.
    pub fields: Document,
    /// Derived columns as sent (used only as the initial value of a new row).
    pub derived: Document,
    pub created_at: Option<BsonDateTime>,
    pub version: i64,
    /// `deletedAt` carried by an upsert payload (a tombstoned row).
    pub deleted_at: Option<BsonDateTime>,
}
