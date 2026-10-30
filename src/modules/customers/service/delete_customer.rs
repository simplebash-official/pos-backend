use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::customers::Customer;

use crate::modules::customers::repository;
use axum::http::StatusCode;

pub async fn delete_customer(
    db: &Db,
    id_or_key: &str,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Customer> {
    crate::core::logging::domain::tracked("customers.deleted", async move {
        let existing = repository::find_customer_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
            })?;

        let object_id = existing
            .id
            .expect("persisted customer document must have an _id");

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This customer was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": existing.into_customer(),
                }),
            ));
        }

        if existing.outstanding_balance_cents != 0 {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::CUSTOMER_HAS_OUTSTANDING_BALANCE,
                format!(
                    "Cannot delete customer '{}' with an outstanding balance of {} cents",
                    existing.name, existing.outstanding_balance_cents
                ),
            ));
        }

        let deleted = repository::delete_customer(db, object_id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
            })?;

        Ok(deleted.into_customer())
    })
    .await
}
