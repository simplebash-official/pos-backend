use std::env;

use crate::{
    clients::db::Db,
    core::{constants::codes, error::AppError},
    domain::users::{CreateUserRequest, Role},
    modules::users::service::create_user,
};

#[derive(Debug, Clone)]
pub struct AdminSeedResult {
    pub email: String,
    pub created: bool,
    pub message: String,
}

/// Bootstraps the Admin account with default credentials `admin@pos.com` / `admin@1234`.
/// Safe to run repeatedly (idempotent): if an admin account already exists, it leaves it unchanged.
pub async fn seed_admin(
    db: &Db,
    email_override: Option<&str>,
    password_override: Option<&str>,
    name_override: Option<&str>,
) -> Result<AdminSeedResult, AppError> {
    let email = email_override
        .map(String::from)
        .or_else(|| env::var("SEED_ADMIN_EMAIL").ok())
        .unwrap_or_else(|| "admin@pos.com".to_string());

    let password = password_override
        .map(String::from)
        .or_else(|| env::var("SEED_ADMIN_PASSWORD").ok())
        .unwrap_or_else(|| "admin@1234".to_string());

    let name = name_override
        .map(String::from)
        .or_else(|| env::var("SEED_ADMIN_NAME").ok())
        .unwrap_or_else(|| "System Admin".to_string());

    let result = create_user(
        db,
        CreateUserRequest {
            name,
            email: email.clone(),
            password,
            role: Role::Admin,
            employee_key: None,
        },
    )
    .await;

    match result {
        Ok(_) => Ok(AdminSeedResult {
            email: email.clone(),
            created: true,
            message: format!("seeded admin account: {email}"),
        }),
        Err(AppError::Custom { code, .. })
            if code == codes::EMAIL_ALREADY_EXISTS || code == codes::ADMIN_ALREADY_EXISTS =>
        {
            Ok(AdminSeedResult {
                email: email.clone(),
                created: false,
                message: format!("admin account already exists ({email}) — no changes made"),
            })
        }
        Err(err) => Err(err),
    }
}
