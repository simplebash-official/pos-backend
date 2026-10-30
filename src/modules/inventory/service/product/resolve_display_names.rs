use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::repository;

/// Resolves display names for a single product's `category_key`/
/// `subcategory_key` for a read-only response — unlike `ensure_valid_category`,
/// this never fails; a key that no longer resolves (e.g. stale data) just
/// falls back to displaying the key itself rather than 500ing a read.
pub(super) async fn resolve_display_names(
    db: &Db,
    category_key: &str,
    subcategory_key: &str,
) -> AppResult<(String, String)> {
    let category_name = repository::category::find_category_by_key(db, category_key)
        .await?
        .map(|document| document.name)
        .unwrap_or_else(|| category_key.to_string());
    let subcategory_name = repository::subcategory::find_subcategory_by_key(db, subcategory_key)
        .await?
        .map(|document| document.name)
        .unwrap_or_else(|| subcategory_key.to_string());

    Ok((category_name, subcategory_name))
}
