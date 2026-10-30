/// The owner login handed over with a shop: the fixed `admin` username, the
/// owner's name and the Argon2id hash of the admin password they chose (see `users::service::create_owner_admin_if_absent`).
#[derive(Debug, Clone)]
pub struct ProvisionOwner {
    pub username: String,
    pub name: String,
    pub password_hash: String,
}
