use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// How a idempotency is stored in the database.
pub struct IdempotencyDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Client-supplied idempotency key string.
    pub key: String,
    /// User ID of the caller submitting the idempotent request.
    pub user_id: String,
    /// SHA-256 hash of the request method, URI, and payload.
    pub request_hash: String,
    /// State of processing ("in_progress" | "completed").
    pub status: String,
    /// HTTP status code of cached response, once completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
    /// Raw response body string of cached response, once completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Timestamp when idempotency record was created.
    pub created_at: BsonDateTime,
}

/// Largest request body this middleware will buffer (to hash it and replay
/// it to the handler). Matches axum's default `DefaultBodyLimit`; the only
/// route that legitimately takes more (`/api/backup`) is exempt below. A
/// bigger body is answered 413 without being read into memory.
pub const MAX_IDEMPOTENT_BODY_BYTES: usize = 2 * 1024 * 1024;
