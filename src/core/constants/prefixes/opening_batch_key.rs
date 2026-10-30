use super::stock_batch::STOCK_BATCH;

/// Key of the batch the migration to stock batches gives every product for the
/// stock (and price) it held before batches existed. Deterministic, so the
/// desktop's SQLite migration and the cloud's Mongo migration create the same
/// row and sync merges them instead of duplicating the stock.
#[must_use]
pub fn opening_batch_key(product_key: &str) -> String {
    format!("{STOCK_BATCH}_open_{product_key}")
}
