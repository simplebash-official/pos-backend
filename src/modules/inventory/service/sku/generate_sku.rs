use super::sku_prefix::sku_prefix;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::inventory::repository;

/// Generates the next SKU for a product being created under
/// `category`/`subcategory`, e.g. "PHO-SCR-0001". A linked device first draws
/// from a block the cloud pre-reserved for this exact prefix (see
/// `reserve_sku_block`/`sync::blocks`), so two offline devices creating
/// products under the same category+subcategory never mint the same SKU;
/// otherwise (unlinked, or the block is exhausted) the numeric suffix comes
/// from the atomic per-prefix counter directly — safe under concurrent
/// creates and needs no uniqueness retry loop, since both paths increment
/// the exact same counter row (see `reserve_sku_block`). Callers should
/// validate `category`/`subcategory` before calling this — a discarded
/// sequence number for a rejected request is otherwise harmless but wasteful.
pub(crate) async fn generate_sku(db: &Db, category: &str, subcategory: &str) -> AppResult<String> {
    let prefix = sku_prefix(category, subcategory);

    if let Some(pool) = db.as_sqlite()
        && let Some(taken) = crate::modules::sync::blocks::next_number(
            pool,
            &format!("sku:{prefix}"),
            &format!("{prefix}-"),
            4,
        )
        .await?
    {
        return Ok(format!("{}{:04}", taken.prefix, taken.number));
    }

    let seq = repository::sku_counter::next_sequence(db, &prefix).await?;
    Ok(format!("{prefix}-{seq:04}"))
}
