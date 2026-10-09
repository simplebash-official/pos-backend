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
    modules::{
        system::repository, tenants::repository as tenants_repository,
        users::service as users_service,
    },
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

/// Evaluates setup status for a specific tenant in multi-tenant mode.
pub(crate) async fn get_tenant_setup_status(
    db: &Db,
    shop_code_or_key: &str,
) -> AppResult<Option<SetupStatusResponse>> {
    let tenant_opt = if shop_code_or_key.starts_with("tnt_") {
        tenants_repository::find_tenant_by_key(db, shop_code_or_key).await?
    } else {
        tenants_repository::find_tenant_by_shop_code(db, shop_code_or_key).await?
    };

    if let Some(tenant) = tenant_opt {
        let app_version = env::var("APP_VERSION").unwrap_or_else(|_| "0.7.0".to_string());
        Ok(Some(SetupStatusResponse {
            setup_completed: tenant.setup_completed,
            is_first_run: !tenant.setup_completed,
            installation_id: Some(tenant.key),
            installed_at: Some(tenant.created_at.to_chrono().to_rfc3339()),
            setup_completed_at: tenant
                .setup_completed_at
                .map(|d| d.to_chrono().to_rfc3339()),
            sample_data_loaded: Some(tenant.sample_data_loaded),
            app_version,
            platform: "Cloud (Multi-Tenant)".to_string(),
        }))
    } else {
        Ok(None)
    }
}

/// Loads the demo data. Suppliers, customers and the catalog do not depend on each other, so they
/// are seeded at the same time (on this task, so a tenant scope still applies); each one is mostly
/// waiting on the database, which is what made a web setup take minutes when done one by one.
async fn seed_sample_data(db: &Db) -> AppResult<()> {
    let catalog = async {
        seeds::providers::seed_providers(db).await?;
        seeds::inventory::seed_inventory(db).await
    };
    tokio::try_join!(
        catalog,
        seeds::suppliers::seed_suppliers(db),
        seeds::customers::seed_customers(db),
    )?;
    Ok(())
}

/// Executes tenant-scoped setup in multi-tenant mode, optionally populating demo data into the tenant namespace.
pub(crate) async fn perform_tenant_setup(
    db: &Db,
    tenant_key: &str,
    load_sample_data: bool,
    admin_user: Option<crate::domain::users::User>,
) -> AppResult<SetupSystemResponse> {
    crate::core::logging::domain::tracked("system.tenant_setup_performed", async move {
        let tenant = tenants_repository::find_tenant_by_key(db, tenant_key)
            .await?
            .ok_or_else(|| AppError::not_found("Tenant not found"))?;

        if tenant.setup_completed {
            return Err(AppError::conflict(
                codes::SETUP_ALREADY_COMPLETED,
                "Tenant setup has already been completed",
            ));
        }

        let scope = crate::core::tenancy::Tenant::id(tenant_key)?;
        crate::core::tenancy::with_tenant(scope, async {
            if load_sample_data {
                seed_sample_data(db).await?;
            }
            Ok::<(), AppError>(())
        })
        .await?;

        tenants_repository::complete_tenant_setup(db, tenant_key, load_sample_data).await?;

        let admin_username = admin_user
            .as_ref()
            .map(|u| u.username.clone())
            .unwrap_or_else(|| roles::ADMIN_USERNAME.to_string());

        Ok(SetupSystemResponse {
            setup_completed: true,
            sample_data_loaded: load_sample_data,
            admin_username,
            token: None,
            user: admin_user,
            message: if load_sample_data {
                "Shop setup completed successfully with sample demo data".to_string()
            } else {
                "Shop setup completed successfully with clean database".to_string()
            },
        })
    })
    .await
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

/// Retrieves the complete installation metadata record for system inspection.
pub(crate) async fn get_installation_info(db: &Db) -> AppResult<SystemInstallation> {
    get_or_create_installation(db).await
}
