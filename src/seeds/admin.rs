use std::env;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, roles::ADMIN_USERNAME},
        error::AppError,
    },
    domain::users::{CreateUserRequest, Role},
    modules::users::service::create_user,
};

#[derive(Debug, Clone)]
pub struct AdminSeedResult {
    pub username: String,
    pub created: bool,
    pub message: String,
}

/// Bootstraps the Admin account: username `admin` (always), password from
/// `SEED_ADMIN_PASSWORD` (non-production runs fall back to `admin@1234`).
/// Safe to run repeatedly (idempotent): if an admin account already exists, it leaves it unchanged.
pub async fn seed_admin(
    db: &Db,
    password_override: Option<&str>,
    name_override: Option<&str>,
) -> Result<AdminSeedResult, AppError> {
    let is_prod = env::var("APP_ENV")
        .or_else(|_| env::var("ENVIRONMENT"))
        .map(|v| {
            let v = v.trim();
            v.eq_ignore_ascii_case("production") || v.eq_ignore_ascii_case("prod")
        })
        .unwrap_or(false);

    let password = match password_override
        .map(String::from)
        .or_else(|| env::var("SEED_ADMIN_PASSWORD").ok())
    {
        Some(p) if !p.trim().is_empty() => p,
        _ if is_prod => {
            return Err(AppError::validation(
                "SEED_ADMIN_PASSWORD is required in production mode; refusing to seed insecure default admin credentials",
            ));
        }
        _ => "admin@1234".to_string(),
    };

    let name = name_override
        .map(String::from)
        .or_else(|| env::var("SEED_ADMIN_NAME").ok())
        .unwrap_or_else(|| "System Admin".to_string());

    let result = create_user(
        db,
        CreateUserRequest {
            name,
            username: ADMIN_USERNAME.to_string(),
            password,
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
