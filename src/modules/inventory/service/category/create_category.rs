use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::{CategoryInfo, CreateCategoryRequest};
use crate::modules::inventory::model::{CategoryDocument, SubcategoryDocument};
use crate::modules::inventory::repository;
use axum::http::StatusCode;
use mongodb::bson::DateTime as BsonDateTime;

/// Validates required fields and name uniqueness, inserts the category, then
/// inserts one `SubcategoryDocument` per name in `body.subcategories` so a
/// category can be bootstrapped with its starter subcategories in one call.
pub async fn create_category(db: &Db, body: CreateCategoryRequest) -> AppResult<CategoryInfo> {
    crate::core::logging::domain::tracked("inventory.category_created", async move {
        if body.name.trim().is_empty() {
            return Err(AppError::validation("Category name is required"));
        }
        if body.icon.trim().is_empty() {
            return Err(AppError::validation("Icon is required"));
        }
        if body.color.trim().is_empty() {
            return Err(AppError::validation("Color is required"));
        }

        if repository::category::find_category_by_name(db, &body.name)
            .await?
            .is_some()
        {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::CATEGORY_ALREADY_EXISTS,
                format!("A category named '{}' already exists", body.name),
            ));
        }

        let now = BsonDateTime::now();
        let category_key = generate_id(prefixes::CATEGORY);
        let document = CategoryDocument {
            id: None,
            key: category_key.clone(),
            name: body.name,
            icon: body.icon,
            color: body.color,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };
        repository::category::insert_category(db, &document).await?;

        let mut subcategories = Vec::with_capacity(body.subcategories.len());
        for name in body.subcategories {
            if name.trim().is_empty() {
                continue;
            }
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
            subcategories.push(subcategory_document.into_subcategory_info());
        }

        Ok(document.into_category_info(subcategories))
    })
    .await
}
