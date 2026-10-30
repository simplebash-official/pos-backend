use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionOwnerRequest {
    /// Always `admin`; the shop code + this username is the POS login.
    pub username: String,
    pub name: String,
    /// Argon2id PHC string of the admin password the owner chose for this shop (`$argon2id$...`), never a plaintext password.
    pub password_hash: String,
}
