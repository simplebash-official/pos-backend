use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::inventory::repository;

/// Reserves a contiguous block of `block_size` SKU numbers under `prefix`
/// (the derived category+subcategory code, e.g. "PHO-SCR" — callers resolve
/// this the same way `generate_sku` does, via `derive_code`) for a linked
/// device to draw individual SKUs from offline. Returns the
/// `(prefix, padding, start, end)` a caller formats with
/// `format!("{prefix}{n:0padding$}")`, matching `generate_sku`'s own output
/// exactly (`prefix` here already carries the trailing `-`).
pub(crate) async fn reserve_sku_block(
    db: &Db,
    prefix: &str,
    block_size: i64,
) -> AppResult<(String, usize, i64, i64)> {
    let (start, end) = repository::sku_counter::reserve_block(db, prefix, block_size).await?;
    Ok((format!("{prefix}-"), 4, start, end))
}
