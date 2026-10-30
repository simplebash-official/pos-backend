use super::resolve_display_names::resolve_display_names;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::Product;

use crate::modules::inventory::repository;

/// Same as `get_product`, looked up by `key` instead of `ObjectId` — the
/// cross-module entry point `supplier_products`/`purchases` call to validate
/// a `productKey` and to enrich their own responses with product display
/// data, since those modules can't reach `inventory::repository` directly
/// (only `inventory::service` is `pub`).
pub(crate) async fn get_product_by_key(db: &Db, key: &str) -> AppResult<Product> {
    let document = repository::product::find_product_by_key(db, key)
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
