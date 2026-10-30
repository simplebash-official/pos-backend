use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::modules::inventory::repository;

/// Checks `subcategory_key` names a subcategory that both exists and
/// belongs to `category_key` — the DB-backed single source of truth for
/// both the category API and this validation. Returns the resolved
/// category/subcategory names (needed for SKU generation) so callers don't
/// have to look them up a second time. Like the rest of this codebase,
/// there's no transaction wrapping this check and the subsequent product
/// write — a category/subcategory deleted in between is an accepted,
/// pre-existing class of risk here, not one this validation newly guards
/// against.
pub(super) async fn ensure_valid_category(
    db: &Db,
    category_key: &str,
    subcategory_key: &str,
) -> AppResult<(String, String)> {
    let category = repository::category::find_category_by_key(db, category_key)
        .await?
        .ok_or_else(|| AppError::validation(format!("'{category_key}' is not a valid category")))?;

    let subcategory = repository::subcategory::find_subcategory_by_key(db, subcategory_key)
        .await?
        .filter(|document| document.category_key == category_key)
        .ok_or_else(|| {
            AppError::validation(format!(
                "'{subcategory_key}' is not a valid subcategory of '{category_key}'"
            ))
        })?;

    Ok((category.name, subcategory.name))
}
