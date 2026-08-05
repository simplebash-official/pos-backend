// Mongo document shape for user accounts. Kept separate from
// `domain::users` (the API-facing types) so `password_hash` and BSON
// concerns like `ObjectId` never leak into request/response payloads —
// `into_user()` deliberately has no way to carry the hash forward.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::users::{Role, User},
};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: Role,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
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
            is_active: self.is_active,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
