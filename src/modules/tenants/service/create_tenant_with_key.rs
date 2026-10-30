use super::normalize_shop_code::normalize_shop_code;
use super::tenant_info::TenantInfo;
use super::validate_tenant_key::validate_tenant_key;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::tenants::model::TenantDocument;
use crate::modules::tenants::repository;

use mongodb::bson::DateTime as BsonDateTime;

/// Registers a tenant under an existing tenant id - the id the identity service
/// issued for the account's shop - so a staff login by shop code lands in the
/// same tenant whose data the devices sync. The id must look like `tnt_...`.
pub async fn create_tenant_with_key(
    db: &Db,
    key: String,
    shop_code: &str,
    name: &str,
) -> AppResult<TenantInfo> {
    crate::core::logging::domain::tracked("tenants.created", async move {
        validate_tenant_key(&key)?;
        let shop_code = normalize_shop_code(shop_code)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("Tenant name is required"));
        }
        let taken = || {
            AppError::conflict(
                codes::SHOP_CODE_ALREADY_EXISTS,
                format!("Shop code '{shop_code}' is already in use"),
            )
        };
        if repository::find_tenant_by_shop_code(db, &shop_code)
            .await?
            .is_some()
        {
            return Err(taken());
        }
        let document = TenantDocument {
            id: None,
            key,
            shop_code: shop_code.clone(),
            name: name.to_string(),
            created_at: BsonDateTime::now(),
            updated_at: BsonDateTime::now(),
            setup_completed: false,
            sample_data_loaded: false,
            setup_completed_at: None,
            suspended_at: None,
            suspended_reason: None,
        };
        // A concurrent creator can pass the check above; the unique index then
        // rejects the second insert.
        if repository::insert_tenant(db, &document).await.is_err()
            && repository::find_tenant_by_shop_code(db, &shop_code)
                .await?
                .is_some()
        {
            return Err(taken());
        }
        Ok(TenantInfo::from(document))
    })
    .await
}
