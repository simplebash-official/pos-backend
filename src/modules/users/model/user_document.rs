use crate::core::constants::{prefixes, roles};
use crate::core::id::generate_id;
use crate::domain::users::{Role, User, UserPreferences};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
    /// Login username, unique within the shop (lowercase).
    pub username: String,
    /// Argon2id hashed password string.
    pub password_hash: String,
    /// Argon2id hashed PIN string for register overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_hash: Option<String>,
    /// System role assigned to this account (Admin, Manager, or Staff).
    pub role: Role,
    /// Whether the user account is enabled and allowed to log in.
    #[serde(default = "default_true")]
    pub is_active: bool,
    /// Key of the `Employee` HR/commission profile this login belongs to,
    /// if any — see `domain::users::User`'s doc comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    /// Per-login UI preferences; absent on legacy documents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<UserPreferences>,
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
            username: self.username,
            role: self.role,
            permissions: roles::default_permissions(self.role)
                .iter()
                .map(|p| p.to_string())
                .collect(),
            is_active: self.is_active,
            has_pin: self.pin_hash.is_some(),
            preferences: self.preferences.unwrap_or_default(),
            employee_key: self.employee_key,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
