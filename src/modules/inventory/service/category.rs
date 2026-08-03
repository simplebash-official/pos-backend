use std::collections::HashMap;

use axum::http::StatusCode;
use mongodb::{Database, bson::doc};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        CategoriesResponse, CategoryInfo, CreateCategoryRequest, SubcategoriesResponse,
        UpdateCategoryRequest, ValidCategoriesResponse,
    },
    modules::inventory::{model::CategoryDocument, repository},
};

pub(crate) async fn list_categories(db: &Database) -> AppResult<CategoriesResponse> {
    let documents = repository::category::list_categories(db).await?;
    let categories = documents
        .into_iter()
        .map(CategoryDocument::into_category_info)
        .collect();

    Ok(CategoriesResponse { categories })
}

pub(crate) async fn create_category(
    db: &Database,
    body: CreateCategoryRequest,
) -> AppResult<CategoryInfo> {
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

    let document = CategoryDocument {
        id: None,
        key: generate_id(prefixes::CATEGORY),
        name: body.name,
        icon: body.icon,
        color: body.color,
        subcategories: body.subcategories,
    };
    repository::category::insert_category(db, &document).await?;

    Ok(document.into_category_info())
}

pub(crate) async fn update_category(
    db: &Database,
    category: String,
    body: UpdateCategoryRequest,
) -> AppResult<(CategoryInfo, u64)> {
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

    let existing = repository::category::find_category_by_name(db, &category)
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
        &category,
        doc! { "name": &new_name, "icon": icon, "color": color },
    )
    .await?
    .ok_or_else(|| {
        AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
    })?;

    // Products reference categories by name, not id — cascade the rename so
    // they don't silently point at a category that no longer exists.
    let renamed_product_count = if new_name != category {
        repository::product::rename_products_category(db, &category, &new_name).await?
    } else {
        0
    };

    Ok((updated.into_category_info(), renamed_product_count))
}

pub(crate) async fn delete_category(db: &Database, category: String) -> AppResult<CategoryInfo> {
    let products_using_category =
        repository::product::count_products_in_category(db, &category).await?;

    if products_using_category > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::CATEGORY_IN_USE,
            format!("{products_using_category} product(s) still reference category '{category}'"),
        ));
    }

    let deleted = repository::category::delete_category(db, &category)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    Ok(deleted.into_category_info())
}

pub(crate) async fn get_valid_categories(db: &Database) -> AppResult<ValidCategoriesResponse> {
    let documents = repository::category::list_categories(db).await?;

    let mut valid_categories = Vec::new();
    let mut category_subcategory_map = HashMap::new();
    for document in documents {
        valid_categories.push(document.name.clone());
        category_subcategory_map.insert(document.name, document.subcategories);
    }

    Ok(ValidCategoriesResponse {
        valid_categories,
        category_subcategory_map,
    })
}

pub(crate) async fn get_category_subcategories(
    db: &Database,
    category: String,
) -> AppResult<SubcategoriesResponse> {
    let document = repository::category::find_category_by_name(db, &category)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    Ok(SubcategoriesResponse {
        category,
        subcategories: document.subcategories,
    })
}

pub(crate) async fn add_subcategory(
    db: &Database,
    category: String,
    subcategory: String,
) -> AppResult<CategoryInfo> {
    if subcategory.trim().is_empty() {
        return Err(AppError::validation("Subcategory name is required"));
    }

    let existing = repository::category::find_category_by_name(db, &category)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    if existing.subcategories.iter().any(|s| s == &subcategory) {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SUBCATEGORY_ALREADY_EXISTS,
            format!("'{subcategory}' already exists under '{category}'"),
        ));
    }

    let updated = repository::category::add_subcategory(db, &category, &subcategory)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    Ok(updated.into_category_info())
}

pub(crate) async fn remove_subcategory(
    db: &Database,
    category: String,
    subcategory: String,
) -> AppResult<CategoryInfo> {
    let existing = repository::category::find_category_by_name(db, &category)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    if !existing.subcategories.iter().any(|s| s == &subcategory) {
        return Err(AppError::not_found_with_code(
            "Subcategory not found",
            codes::SUBCATEGORY_NOT_FOUND,
        ));
    }

    let products_using_subcategory =
        repository::product::count_products_in_subcategory(db, &category, &subcategory).await?;

    if products_using_subcategory > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SUBCATEGORY_IN_USE,
            format!(
                "{products_using_subcategory} product(s) still reference subcategory '{subcategory}'"
            ),
        ));
    }

    let updated = repository::category::remove_subcategory(db, &category, &subcategory)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Category not found", "CATEGORY_NOT_FOUND"))?;

    Ok(updated.into_category_info())
}
