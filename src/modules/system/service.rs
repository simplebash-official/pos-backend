// Business logic for initial system setup, onboarding, and conditional database seeding.

use chrono::Utc;
use jsonwebtoken::{EncodingKey, Header, encode};
use std::env;

use crate::{
    clients::db::Db,
    core::{
        config::Config,
        constants::{codes, roles},
        error::{AppError, AppResult},
        id::generate_id,
        middleware::auth::Claims,
    },
    domain::system::{
        SetupStatusResponse, SetupSystemRequest, SetupSystemResponse, SystemInstallation,
    },
    modules::{system::repository, users::service as users_service},
    seeds,
};

/// Retrieves the existing installation record, or creates and persists a new one.
pub(crate) async fn get_or_create_installation(db: &Db) -> AppResult<SystemInstallation> {
    if let Some(existing) = repository::get_installation(db).await? {
        return Ok(existing);
    }

    let installation_id =
        env::var("INSTALLATION_ID").unwrap_or_else(|_| format!("inst_{}", nanoid::nanoid!(16)));
    let app_version = env::var("APP_VERSION").unwrap_or_else(|_| "0.5.0".to_string());
    let platform = env::var("PLATFORM").unwrap_or_else(|_| std::env::consts::OS.to_string());
    let now = Utc::now().to_rfc3339();

    let new_inst = SystemInstallation {
        key: generate_id("inst"),
        id: generate_id("inst"),
        installation_id,
        app_version,
        platform,
        installed_at: now.clone(),
        setup_completed: false,
        setup_completed_at: None,
        sample_data_loaded: false,
        created_at: now.clone(),
        updated_at: now,
    };

    repository::create_installation(db, &new_inst).await?;
    Ok(new_inst)
}

/// Evaluates whether initial system setup has been completed or is required.
pub(crate) async fn get_setup_status(db: &Db) -> AppResult<SetupStatusResponse> {
    let installation = get_or_create_installation(db).await?;
    let users_count = repository::count_users(db).await?;

    // Invariant: setup is truly completed only if marked in installation AND at least one user exists.
    let setup_completed = installation.setup_completed && users_count > 0;
    let is_first_run = !setup_completed && users_count == 0;

    Ok(SetupStatusResponse {
        setup_completed,
        is_first_run,
        installation_id: Some(installation.installation_id),
        installed_at: Some(installation.installed_at),
        setup_completed_at: installation.setup_completed_at,
        sample_data_loaded: Some(installation.sample_data_loaded),
        app_version: installation.app_version,
        platform: installation.platform,
    })
}

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
        // takeover risk on every fresh install, so the caller must choose both.
        let admin_email = body
            .admin_email
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::validation("Admin email is required"))?;

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

        // 1. Bootstrap the Admin user account
        seeds::admin::seed_admin(
            db,
            Some(admin_email),
            Some(admin_password),
            Some(admin_name),
        )
        .await?;

        // 2. Bootstrap the default API key
        seeds::api_key::seed_api_key(db).await?;

        // 3. Conditional Seeding: if user requested sample data, seed categories, inventory, suppliers & customers
        if body.load_sample_data {
            seeds::providers::seed_providers(db).await?;
            seeds::suppliers::seed_suppliers(db).await?;
            seeds::customers::seed_customers(db).await?;
            seeds::inventory::seed_inventory(db).await?;
        }

        // 4. Mark setup completed in database
        repository::complete_installation(db, &installation.installation_id, body.load_sample_data)
            .await?;

        // 5. Verify credentials & issue JWT token for immediate auto-login
        let user = users_service::verify_credentials(db, admin_email, admin_password).await?;
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
            admin_email: admin_email.to_string(),
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

/// Retrieves the complete installation metadata record for system inspection.
pub(crate) async fn get_installation_info(db: &Db) -> AppResult<SystemInstallation> {
    get_or_create_installation(db).await
}
