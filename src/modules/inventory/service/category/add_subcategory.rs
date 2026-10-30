use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::CategoryInfo;
use crate::modules::inventory::model::SubcategoryDocument;
use crate::modules::inventory::repository;
use axum::http::StatusCode;
use mongodb::bson::DateTime as BsonDateTime;

/// Adds a subcategory under a category, rejecting duplicate names within
/// that same category (409 `SUBCATEGORY_ALREADY_EXISTS`).
pub async fn add_subcategory(
    db: &Db,
    category_key: String,
    name: String,
) -> AppResult<CategoryInfo> {
    crate::core::logging::domain::tracked("inventory.subcategory_added", async move {
        if name.trim().is_empty() {
            return Err(AppError::validation("Subcategory name is required"));
        }

        let category = repository::category::find_category_by_key(db, &category_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
            })?;

        if repository::subcategory::find_subcategory_by_category_and_name(db, &category_key, &name)
            .await?
            .is_some()
        {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::SUBCATEGORY_ALREADY_EXISTS,
                format!("'{name}' already exists under '{}'", category.name),
            ));
        }

        let now = BsonDateTime::now();
        let subcategory_document = SubcategoryDocument {
            id: None,
            key: generate_id(prefixes::SUBCATEGORY),
            category_key: category_key.clone(),
            name,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };
        repository::subcategory::insert_subcategory(db, &subcategory_document).await?;

        let subcategory_documents =
            repository::subcategory::list_subcategories_by_category(db, &category_key).await?;
        let subcategories = subcategory_documents
            .into_iter()
            .map(SubcategoryDocument::into_subcategory_info)
            .collect();

        Ok(category.into_category_info(subcategories))
    })
    .await
}
