use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::Product;

use crate::modules::inventory::repository;

use mongodb::bson::doc;

/// Cross-module batch lookup — `purchases::service::purchase::list_purchases`
/// calls this (never `inventory::repository` directly, which is private to
/// this module) to enrich a page of purchase history with product display
/// data in one query instead of one `get_product_by_key` call per row.
pub(crate) async fn get_products_by_keys(db: &Db, keys: &[String]) -> AppResult<Vec<Product>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let filter = doc! { "key": { "$in": keys } };
    let (items, _total) = repository::product::list_products_with_display_names(
        db,
        filter,
        doc! {},
        0,
        keys.len() as i64,
    )
    .await?;
    let mut items = items;
    super::super::stock_batch::attach_price_tiers(db, &mut items).await?;
    Ok(items)
}
