use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::system::SetupStatusResponse;
use crate::modules::tenants::repository as tenants_repository;
use std::env;

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
