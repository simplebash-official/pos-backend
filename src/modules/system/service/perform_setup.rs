use super::get_or_create_installation::get_or_create_installation;
use super::seed_sample_data::seed_sample_data;
use crate::clients::db::Db;
use crate::core::config::Config;
use crate::core::constants::{codes, roles};
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::Claims;
use crate::domain::system::{SetupSystemRequest, SetupSystemResponse};
use crate::modules::system::repository;
use crate::modules::users::service as users_service;
use crate::seeds;
use chrono::Utc;
use jsonwebtoken::{EncodingKey, Header, encode};

/// Executes initial system setup, bootstrapping the administrator and optionally loading demo data.
pub(crate) async fn perform_setup(
    db: &Db,
    config: &Config,
    body: SetupSystemRequest,
) -> AppResult<SetupSystemResponse> {
    crate::core::logging::domain::tracked("system.setup_performed", async move {
        let installation = get_or_create_installation(db).await?;
        let users_count = repository::count_users(db).await?;

        if installation.setup_completed && users_count > 0 {
            return Err(AppError::conflict(
                codes::SETUP_ALREADY_COMPLETED,
                "System setup has already been completed",
            ));
        }

        // No built-in fallback credentials: a public default admin login is a
        // takeover risk on every fresh install, so the caller must choose the password (the username is always `admin`).
        let admin_password = body
            .admin_password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::validation("Admin password is required"))?;
        if admin_password.chars().count() < 8 {
            return Err(AppError::validation(
                "Admin password must be at least 8 characters",
            ));
        }

        let admin_name = body
            .admin_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("System Admin");

        // An account already exists (created by AUTO_SEED or the `seed_admin`
        // binary) but setup was never marked complete. Only the holder of that
        // account may finish setup — check the credentials *before* any side
        // effect (API key, sample data, completion flag), so an anonymous
        // caller can't re-run setup against a live shop.
        if users_count > 0 {
            users_service::verify_credentials(db, roles::ADMIN_USERNAME, admin_password).await?;
        }

        // 1. Bootstrap the Admin user account
        seeds::admin::seed_admin(db, Some(admin_password), Some(admin_name)).await?;

        // 2. Bootstrap the default API key
        seeds::api_key::seed_api_key(db).await?;

        // 3. Conditional Seeding: if user requested sample data, seed categories, inventory, suppliers & customers
        if body.load_sample_data {
            seed_sample_data(db).await?;
        }

        // 4. Mark setup completed in database
        repository::complete_installation(db, &installation.installation_id, body.load_sample_data)
            .await?;

        // 5. Verify credentials & issue JWT token for immediate auto-login
        let user =
            users_service::verify_credentials(db, roles::ADMIN_USERNAME, admin_password).await?;
        let permissions: Vec<String> = roles::default_permissions(user.role)
            .iter()
            .map(|p| p.to_string())
            .collect();
        let exp =
            (Utc::now() + chrono::Duration::hours(config.jwt_expiry_hours)).timestamp() as usize;
        let claims = Claims {
            sub: user.id.clone(),
            exp,
            role: Some(user.role),
            permissions,
            tid: crate::core::tenancy::current_tenant_id(),
            scope: None,
            did: None,
            name: Some(user.name.clone()),
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
        )
        .map_err(|e| AppError::internal(format!("Failed to generate authentication token: {e}")))?;

        Ok(SetupSystemResponse {
            setup_completed: true,
            sample_data_loaded: body.load_sample_data,
            admin_username: roles::ADMIN_USERNAME.to_string(),
            token: Some(token),
            user: Some(user),
            message: if body.load_sample_data {
                "System setup completed successfully with sample demo data".to_string()
            } else {
                "System setup completed successfully with clean database".to_string()
            },
        })
    })
    .await
}
