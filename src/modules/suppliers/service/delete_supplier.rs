use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::Supplier;

use crate::modules::suppliers::repository;
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;

/// Refuses to delete (409 `SUPPLIER_HAS_PURCHASES`) while any purchase
/// record still references this supplier by `key` — purchase history is
/// financial/audit data, so it's blocked from ever pointing at nothing
/// (mirrors `inventory::service::category::delete_category`'s
/// `CATEGORY_IN_USE` guard). Once the guard passes, cascades a delete of
/// every `supplier_products` link for this supplier — those are pure
/// linking metadata, not audit history, so no data of lasting value is lost.
pub(crate) async fn delete_supplier(
    db: &Db,
    id: ObjectId,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Supplier> {
    crate::core::logging::domain::tracked("suppliers.deleted", async move {
        let existing = repository::find_supplier_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This supplier was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": existing.into_supplier(),
                    "conflictingFields": Vec::<String>::new(),
                }),
            ));
        }

        let purchase_count =
            crate::modules::purchases::service::purchase::count_purchases_for_supplier(
                db,
                &existing.key,
            )
            .await?;
        if purchase_count > 0 {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::SUPPLIER_HAS_PURCHASES,
                format!(
                    "{purchase_count} purchase record(s) still reference supplier '{}'",
                    existing.name
                ),
            ));
        }

        crate::modules::supplier_products::service::link::delete_links_for_supplier(
            db,
            &existing.key,
        )
        .await?;

        let deleted = repository::delete_supplier(db, id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        Ok(deleted.into_supplier())
    })
    .await
}
