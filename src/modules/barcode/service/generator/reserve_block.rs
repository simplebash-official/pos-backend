use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::barcode::repository;

/// Reserves a contiguous block of `block_size` barcode sequence numbers under
/// `namespace` for a linked device to draw individual barcodes from offline.
pub(crate) async fn reserve_block(
    db: &Db,
    namespace: &str,
    block_size: i64,
) -> AppResult<(i64, i64)> {
    repository::reserve_block(db, namespace, block_size).await
}
