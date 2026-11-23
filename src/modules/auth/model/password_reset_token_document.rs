//! Persistence model for password reset tokens.
//!
//! Stores single-use tokens hashed with SHA-256 for Admin password recovery.

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Stored representation of a password reset token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordResetTokenDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this reset token (e.g. prt_...).
    pub key: String,
    /// Foreign key referencing the user account.
    pub user_key: String,
    /// SHA-256 hash of the 32-byte cryptographically random token string.
    pub token_hash: String,
    /// Timestamp when this reset token expires.
    pub expires_at: BsonDateTime,
    /// Timestamp when this reset token was consumed, if used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_at: Option<BsonDateTime>,
    /// Timestamp when this reset token was generated.
    pub created_at: BsonDateTime,
    /// Tenant identifier in multi-tenant mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}
