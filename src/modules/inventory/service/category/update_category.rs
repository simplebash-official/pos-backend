use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{CategoryInfo, UpdateCategoryRequest};
use crate::modules::inventory::model::SubcategoryDocument;
use crate::modules::inventory::repository;
use axum::http::StatusCode;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Updates a category's fields, looked up and persisted by its immutable
/// `key`. Renaming (`name`) is now a pure display-label change — since
/// products/subcategories reference this category by `key`, nothing else
/// needs to be touched.
pub async fn update_category(
    db: &Db,
    category_key: String,
    body: UpdateCategoryRequest,
) -> AppResult<CategoryInfo> {
    crate::core::logging::domain::tracked("inventory.category_updated", async move {
        if let Some(name) = &body.name
            && name.trim().is_empty()
        {
            return Err(AppError::validation("Category name cannot be empty"));
        }
        if let Some(icon) = &body.icon
            && icon.trim().is_empty()
        {
            return Err(AppError::validation("Icon cannot be empty"));
        }
        if let Some(color) = &body.color
            && color.trim().is_empty()
        {
            return Err(AppError::validation("Color cannot be empty"));
        }

        let existing = repository::category::find_category_by_key(db, &category_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
            })?;

        let new_name = body.name.unwrap_or_else(|| existing.name.clone());
        if new_name != existing.name
            && repository::category::find_category_by_name(db, &new_name)
                .await?
                .is_some()
        {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::CATEGORY_ALREADY_EXISTS,
                format!("A category named '{new_name}' already exists"),
            ));
        }

        let icon = body.icon.unwrap_or(existing.icon);
        let color = body.color.unwrap_or(existing.color);

        let updated = repository::category::update_category_fields(
            db,
            &category_key,
            doc! { "name": &new_name, "icon": icon, "color": color, "updated_at": BsonDateTime::now() },
        )
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

        let subcategory_documents =
            repository::subcategory::list_subcategories_by_category(db, &category_key).await?;
        let subcategories = subcategory_documents
            .into_iter()
            .map(SubcategoryDocument::into_subcategory_info)
            .collect();

        Ok(updated.into_category_info(subcategories))
    })
    .await
}
