use super::tiers_from_batches::tiers_from_batches;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::Product;
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;

use std::collections::HashMap;

/// Fills `price_tiers` on every product from its in-stock batches, with one
/// batch query for the whole slice. Every product read path calls this
/// before answering (see `ProductDocument::into_product`).
pub(crate) async fn attach_price_tiers<'a>(
    db: &Db,
    products: impl IntoIterator<Item = &'a mut Product>,
) -> AppResult<()> {
    let products: Vec<&mut Product> = products.into_iter().collect();
    let keys: Vec<String> = products.iter().map(|product| product.key.clone()).collect();
    let batches = repository::stock_batch::list_in_stock_batches_for_products(db, &keys).await?;
    let mut by_product: HashMap<&str, Vec<&StockBatchDocument>> = HashMap::new();
    for batch in &batches {
        by_product
            .entry(batch.product_key.as_str())
            .or_default()
            .push(batch);
    }

    let mut out_of_stock_keys: Vec<String> = Vec::new();
    for product in &products {
        if !by_product.contains_key(product.key.as_str()) {
            out_of_stock_keys.push(product.key.clone());
        }
    }

    let latest_batches = if !out_of_stock_keys.is_empty() {
        repository::stock_batch::list_latest_batches_for_products(db, &out_of_stock_keys).await?
    } else {
        Vec::new()
    };
    let mut latest_by_product: HashMap<&str, i64> = HashMap::new();
    for batch in &latest_batches {
        latest_by_product
            .entry(batch.product_key.as_str())
            .or_insert(batch.selling_price_cents);
    }

    for product in products {
        let tiers = by_product
            .get(product.key.as_str())
            .map(|batches| tiers_from_batches(batches.iter().copied()))
            .unwrap_or_default();
        if let Some(last_tier) = tiers.last() {
            product.last_selling_price_cents = Some(last_tier.selling_price_cents);
        } else if let Some(last_price) = latest_by_product.get(product.key.as_str()) {
            product.last_selling_price_cents = Some(*last_price);
        }
        product.price_tiers = tiers;
    }
    Ok(())
}
