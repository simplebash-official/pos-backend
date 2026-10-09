// Mongo document shape for the `login_sessions` audit log — one row per
// successful login. Kept separate from `domain::auth` (the API-facing
// types) so BSON concerns like `ObjectId` never leak into responses.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::{auth::LoginSession, users::Role};

/// `name_at_login`/`username_at_login`/`role_at_login` are a point-in-time
/// snapshot of the account, not a live join against the current user
/// document — deliberately different from how `purchases` resolves its
/// supplier/product references. A login-session record is an audit trail of
/// what was true *at the moment of login*; if the account is later renamed
/// or its role changes, historical entries should keep reading the way they
/// did when the login happened. `user_key` is still stored (the immutable
/// FK, per the Custom Prefixed Unique Model Keys rule) so a session can
/// still be correlated back to the current account if needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginSessionDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique human-readable business key for this login session.
    pub key: String,
    /// Foreign key referencing the user who logged in.
    pub user_key: String,
    /// User's display name at the moment they logged in.
    pub name_at_login: String,
    /// User's username at the moment they logged in.
    pub username_at_login: String,
    /// User's assigned role at the moment they logged in.
    pub role_at_login: Role,
    /// IP address where the login request originated from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// Web browser or client software information used to log in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    // `updated_at` is set once at insert and never changes again — this is
    // an append-only log, kept only for consistency with the Collection
    // Document Timestamps rule the rest of this codebase follows.
    /// Timestamp when this login session record was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when this record was last modified.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl LoginSessionDocument {
    pub fn into_login_session(self) -> LoginSession {
        LoginSession {
            id: self
                .id
                .expect("persisted login session document must have an id")
                .to_hex(),
            key: self.key,
            user_key: self.user_key,
            name: self.name_at_login,
            username: self.username_at_login,
            role: self.role_at_login,
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            logged_in_at: self.created_at.to_chrono(),
        }
    }
}
