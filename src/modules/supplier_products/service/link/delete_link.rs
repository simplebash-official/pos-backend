use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::supplier_products::repository;

/// Unlinks a supplier from a product, 404ing with
/// `SUPPLIER_PRODUCT_LINK_NOT_FOUND` if no such link exists.
pub(crate) async fn delete_link(db: &Db, supplier_key: &str, product_key: &str) -> AppResult<()> {
    repository::delete_link(db, supplier_key, product_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                "Supplier-product link not found",
                codes::SUPPLIER_PRODUCT_LINK_NOT_FOUND,
            )
        })?;
    Ok(())
}
