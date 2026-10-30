use crate::core::error::AppResult;

use crate::modules::sync::state::pool_of;

/// Distinct SKU prefixes (e.g. "PHO-SCR") this device's local catalog needs a
/// cloud-reserved block for — one per non-deleted category+subcategory pair,
/// derived the exact same way `inventory::service::sku::generate_sku` would.
/// Called by the sync agent's periodic block-refill check to discover which
/// `sku:<prefix>` blocks to request, since SKU has no single fixed name the
/// way invoice/creditnote/etc. do.
pub(crate) async fn local_sku_prefixes(db: &crate::clients::db::Db) -> AppResult<Vec<String>> {
    let pool = pool_of(db)?;
    let pairs: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.name, s.name FROM subcategories s \
         JOIN categories c ON c.key = s.category_key \
         WHERE s.deleted_at IS NULL AND c.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut prefixes: Vec<String> = pairs
        .into_iter()
        .map(|(category, subcategory)| {
            crate::modules::inventory::service::sku::sku_prefix(&category, &subcategory)
        })
        .collect();
    prefixes.sort();
    prefixes.dedup();
    Ok(prefixes)
}
