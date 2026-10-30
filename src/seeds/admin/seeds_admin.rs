use crate::clients::db::Db;
use crate::core::config::SeedAdminConfig;
use crate::core::constants::codes;
use crate::core::constants::roles::ADMIN_USERNAME;
use crate::core::error::AppError;
use crate::domain::users::{CreateUserRequest, Role};
use crate::modules::users::service::create_user;

#[derive(Debug, Clone)]
/// What seeding admin created and skipped.
pub struct AdminSeedResult {
    pub username: String,
    pub created: bool,
    pub message: String,
}

/// Bootstraps the Admin account: username `admin` (always), with the configured name and password.
/// Safe to run repeatedly (idempotent): if an admin account already exists, it leaves it unchanged.
pub async fn seed_admin(db: &Db, admin: &SeedAdminConfig) -> Result<AdminSeedResult, AppError> {
    let result = create_user(
        db,
        CreateUserRequest {
            name: admin.name.clone(),
            username: ADMIN_USERNAME.to_string(),
            password: admin.password.clone(),
            role: Role::Admin,
            employee_key: None,
        },
    )
    .await;

    match result {
        Ok(_) => Ok(AdminSeedResult {
            username: ADMIN_USERNAME.to_string(),
            created: true,
            message: format!("seeded admin account: {ADMIN_USERNAME}"),
        }),
        Err(AppError::Custom { code, .. })
            if code == codes::USERNAME_ALREADY_EXISTS || code == codes::ADMIN_ALREADY_EXISTS =>
        {
            Ok(AdminSeedResult {
                username: ADMIN_USERNAME.to_string(),
                created: false,
                message: format!(
                    "admin account already exists ({ADMIN_USERNAME}) — no changes made"
                ),
            })
        }
        Err(err) => Err(err),
    }
}
