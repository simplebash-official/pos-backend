use super::seed_sample_data::seed_sample_data;
use crate::clients::db::Db;
use crate::core::constants::{codes, roles};
use crate::core::error::{AppError, AppResult};
use crate::domain::system::SetupSystemResponse;
use crate::modules::tenants::repository as tenants_repository;

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
