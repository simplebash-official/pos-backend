use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::modules::inventory::model::{CategoryDocument, SubcategoryDocument};
use crate::modules::inventory::repository;
use axum::http::StatusCode;
use mongodb::bson::DateTime as BsonDateTime;

/// Key of the shipped "Uncategorized" category. Fixed, so every device and the cloud seed the
/// same row and the billing screen's quick-add form can offer it by key.
pub const UNCATEGORIZED_CATEGORY_KEY: &str = "cat_uncategorized";
/// Key of the one subcategory under [`UNCATEGORIZED_CATEGORY_KEY`].
pub const UNCATEGORIZED_SUBCATEGORY_KEY: &str = "subcat_uncategorized_general";

const CATEGORY_NAME: &str = "Uncategorized";
const SUBCATEGORY_NAME: &str = "General";
const CATEGORY_ICON: &str = "Box";
const CATEGORY_COLOR: &str = "gray";

/// 2026-01-01T00:00:00Z: the fixed time the shipped rows are stamped with, so a real edit made
/// anywhere is newer and a re-seed can never undo it when sync compares them.
const SEEDED_AT_MS: i64 = 1_767_225_600_000;

/// Creates the shipped "Uncategorized / General" category and subcategory when this shop lacks
/// them. Never touches an existing row, so edits survive.
pub async fn ensure_uncategorized(db: &Db) -> AppResult<()> {
    db.require_active_tenant_scope()?;
    let seeded_at = BsonDateTime::from_millis(SEEDED_AT_MS);

    if repository::category::find_category_by_key(db, UNCATEGORIZED_CATEGORY_KEY)
        .await?
        .is_none()
    {
        repository::category::insert_category(
            db,
            &CategoryDocument {
                id: None,
                key: UNCATEGORIZED_CATEGORY_KEY.to_string(),
                name: CATEGORY_NAME.to_string(),
                icon: CATEGORY_ICON.to_string(),
                color: CATEGORY_COLOR.to_string(),
                warranty_template: None,
                version: 1,
                created_at: seeded_at,
                updated_at: seeded_at,
                deleted_at: None,
                updated_by_device: None,
            },
        )
        .await?;
    }

    if repository::subcategory::find_subcategory_by_key(db, UNCATEGORIZED_SUBCATEGORY_KEY)
        .await?
        .is_none()
    {
        repository::subcategory::insert_subcategory(
            db,
            &SubcategoryDocument {
                id: None,
                key: UNCATEGORIZED_SUBCATEGORY_KEY.to_string(),
                category_key: UNCATEGORIZED_CATEGORY_KEY.to_string(),
                name: SUBCATEGORY_NAME.to_string(),
                version: 1,
                created_at: seeded_at,
                updated_at: seeded_at,
                deleted_at: None,
                updated_by_device: None,
            },
        )
        .await?;
    }
    Ok(())
}

/// Refuses (409 `SYSTEM_CATEGORY_PROTECTED`) to remove the shipped category, so the quick-add
/// form always has somewhere to put an item.
pub(super) fn ensure_category_removable(category_key: &str) -> AppResult<()> {
    if category_key == UNCATEGORIZED_CATEGORY_KEY {
        return Err(protected(CATEGORY_NAME));
    }
    Ok(())
}

/// Refuses (409 `SYSTEM_CATEGORY_PROTECTED`) to remove the shipped subcategory.
pub(super) fn ensure_subcategory_removable(subcategory_key: &str) -> AppResult<()> {
    if subcategory_key == UNCATEGORIZED_SUBCATEGORY_KEY {
        return Err(protected(SUBCATEGORY_NAME));
    }
    Ok(())
}

fn protected(name: &str) -> AppError {
    AppError::custom(
        StatusCode::CONFLICT,
        codes::SYSTEM_CATEGORY_PROTECTED,
        format!("'{name}' is built in and cannot be removed"),
    )
}
