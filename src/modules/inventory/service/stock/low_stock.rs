use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::{LowStockItem, LowStockResponse};

use crate::modules::inventory::repository;

/// Products at or below their configured reorder threshold.
pub(crate) async fn low_stock(db: &Db) -> AppResult<LowStockResponse> {
    let documents = repository::product::find_low_stock_products(db).await?;

    let items: Vec<LowStockItem> = documents
        .into_iter()
        .map(|document| LowStockItem {
            id: document
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key: document.key,
            sku: document.sku,
            name: document.name,
            stock_quantity: document.stock_quantity,
            min_stock_threshold: document.min_stock_threshold,
            deficit: document.min_stock_threshold - document.stock_quantity,
        })
        .collect();

    let total = items.len() as u64;

    Ok(LowStockResponse { items, total })
}
