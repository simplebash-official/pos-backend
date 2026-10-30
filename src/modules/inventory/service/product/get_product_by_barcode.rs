use super::resolve_display_names::resolve_display_names;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::Product;

use crate::modules::inventory::repository;

/// Exact barcode lookup for a scan — resolves the one product carrying
/// `barcode`, 404ing with `PRODUCT_NOT_FOUND` when none does. Unlike the
/// `search` list param (a substring regex that can return several rows) this
/// is an anchored equality match, so a scanner gets an unambiguous result.
pub(crate) async fn get_product_by_barcode(db: &Db, barcode: &str) -> AppResult<Product> {
    let document = repository::product::find_product_by_barcode(db, barcode)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    let (category_name, subcategory_name) =
        resolve_display_names(db, &document.category_key, &document.subcategory_key).await?;

    super::super::stock_batch::with_price_tiers(
        db,
        document.into_product(category_name, subcategory_name),
    )
    .await
}
