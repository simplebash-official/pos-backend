use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::tenants::model::TenantDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Marks setup as completed for a tenant.
pub(crate) async fn complete_tenant_setup(
    db: &Db,
    key: &str,
    sample_data_loaded: bool,
) -> AppResult<()> {
    let now = mongodb::bson::DateTime::now();
    directory(db)?
        .update_one(
            doc! { "key": key },
            doc! {
                "$set": {
                    "setup_completed": true,
                    "sample_data_loaded": sample_data_loaded,
                    "setup_completed_at": now,
                    "updated_at": now,
                }
            },
        )
        .await?;
    Ok(())
}

pub(super) fn directory(
    db: &Db,
) -> AppResult<crate::clients::tenant_db::ScopedCollection<TenantDocument>> {
    let mongo = db.as_mongo().ok_or_else(|| {
        AppError::internal("the tenant directory requires MongoDB (TENANT_MODE=multi)")
    })?;
    Ok(mongo.platform_collection::<TenantDocument>("tenants"))
}

/// Finds a tenant by its immutable id (`tnt_...`).
pub(crate) async fn find_tenant_by_key(db: &Db, key: &str) -> AppResult<Option<TenantDocument>> {
    Ok(directory(db)?.find_one(doc! { "key": key }).await?)
}

/// Lists every tenant in the platform directory in creation order.
pub(crate) async fn list_all_tenants(db: &Db) -> AppResult<Vec<TenantDocument>> {
    let cursor = directory(db)?
        .find(doc! {})
        .sort(doc! { "created_at": 1, "key": 1 })
        .await?;
    Ok(cursor.try_collect().await?)
}

/// Finds a tenant by its (already normalised) shop code.
pub(crate) async fn find_tenant_by_shop_code(
    db: &Db,
    shop_code: &str,
) -> AppResult<Option<TenantDocument>> {
    Ok(directory(db)?
        .find_one(doc! { "shop_code": shop_code })
        .await?)
}

/// Inserts a new tenant. The unique `shop_code` index turns a racing duplicate
/// into a driver error, which the caller reports as a conflict.
pub(crate) async fn insert_tenant(db: &Db, document: &TenantDocument) -> AppResult<()> {
    directory(db)?.insert_one(document).await?;
    Ok(())
}

/// Sets (`Some((since, reason))`) or clears (`None`) a tenant's suspension marker. `false` when no
/// such tenant exists.
pub(crate) async fn set_tenant_suspended(
    db: &Db,
    key: &str,
    suspension: Option<(mongodb::bson::DateTime, &str)>,
) -> AppResult<bool> {
    let now = mongodb::bson::DateTime::now();
    let update = match suspension {
        Some((since, reason)) => doc! {
            "$set": { "suspended_at": since, "suspended_reason": reason, "updated_at": now }
        },
        None => doc! {
            "$set": { "updated_at": now },
            "$unset": { "suspended_at": "", "suspended_reason": "" }
        },
    };
    let result = directory(db)?
        .update_one(doc! { "key": key }, update)
        .await?;
    Ok(result.matched_count > 0)
}

/// Renames a tenant (identity is the source of truth for the shop name).
pub(crate) async fn update_tenant_name(db: &Db, key: &str, name: &str) -> AppResult<()> {
    directory(db)?
        .update_one(
            doc! { "key": key },
            doc! { "$set": { "name": name, "updated_at": mongodb::bson::DateTime::now() } },
        )
        .await?;
    Ok(())
}
