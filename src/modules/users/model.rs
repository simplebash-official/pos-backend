// Mongo document shape for user accounts. Kept separate from
// `domain::users` (the API-facing types) so `password_hash` and BSON
// concerns like `ObjectId` never leak into request/response payloads —
// `into_user()` deliberately has no way to carry the hash forward.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{
        constants::{prefixes, roles},
        id::generate_id,
    },
    domain::users::{Role, User},
};

fn default_true() -> bool {
    true
}

/// Mongo document shape representing a user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this user (e.g. usr_...).
    #[serde(default)]
    pub key: String,
    /// Full display name of the user.
    pub name: String,
    /// Unique login email address.
    pub email: String,
    /// Argon2id hashed password string.
    pub password_hash: String,
    /// System role assigned to this account (Admin, Manager, or Staff).
    pub role: Role,
    /// Whether the user account is enabled and allowed to log in.
    #[serde(default = "default_true")]
    pub is_active: bool,
    /// Key of the `Employee` HR/commission profile this login belongs to,
    /// if any — see `domain::users::User`'s doc comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    /// Timestamp when user account was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when user account was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl UserDocument {
    pub fn into_user(self) -> User {
        let key = if self.key.is_empty() {
            generate_id(prefixes::USER)
        } else {
            self.key
        };
        User {
            id: self
                .id
                .expect("persisted user document must have an id")
                .to_hex(),
            key,
            name: self.name,
            email: self.email,
            role: self.role,
            permissions: roles::default_permissions(self.role)
                .iter()
                .map(|p| p.to_string())
                .collect(),
            is_active: self.is_active,
            employee_key: self.employee_key,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
