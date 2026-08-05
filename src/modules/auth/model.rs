// Mongo document shape for the `login_sessions` audit log — one row per
// successful login. Kept separate from `domain::auth` (the API-facing
// types) so BSON concerns like `ObjectId` never leak into responses.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::{auth::LoginSession, users::Role};

/// `name_at_login`/`email_at_login`/`role_at_login` are a point-in-time
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
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub user_key: String,
    pub name_at_login: String,
    pub email_at_login: String,
    pub role_at_login: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    // `updated_at` is set once at insert and never changes again — this is
    // an append-only log, kept only for consistency with the Collection
    // Document Timestamps rule the rest of this codebase follows.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
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
            email: self.email_at_login,
            role: self.role_at_login,
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            logged_in_at: self.created_at.to_chrono(),
        }
    }
}
