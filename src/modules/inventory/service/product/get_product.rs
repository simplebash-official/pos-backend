use super::resolve_display_names::resolve_display_names;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::Product;

use crate::modules::inventory::repository;

use mongodb::bson::oid::ObjectId;

/// Fetch by id, 404ing with the module-specific `PRODUCT_NOT_FOUND` code
/// rather than the generic `NOT_FOUND`.
pub(crate) async fn get_product(db: &Db, id: ObjectId) -> AppResult<Product> {
    let document = repository::product::find_product_by_id(db, id)
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
