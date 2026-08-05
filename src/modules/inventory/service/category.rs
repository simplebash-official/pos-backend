// Business rules for category/subcategory management: uniqueness checks
// and the "still referenced by products" guards that block deleting a
// category/subcategory while products still point at it by key. Renaming a
// category's `name` no longer cascades anywhere — products reference a
// category/subcategory by its immutable `key`, not its (renameable) name.

use axum::http::StatusCode;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, doc},
};
use std::collections::HashMap;

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        CategoriesResponse, CategoryInfo, CreateCategoryRequest, SubcategoriesResponse,
        SubcategoryInfo, UpdateCategoryRequest, ValidCategoriesResponse, ValidCategoryOption,
        ValidSubcategoryOption,
    },
    modules::inventory::{
        model::{CategoryDocument, SubcategoryDocument},
        repository,
    },
};

/// All categories with their subcategories, alphabetically sorted (see
/// `repository::category::list_categories`). Fetches every subcategory in
/// one extra query and groups by `category_key` in memory, rather than one
/// subcategory query per category.
pub(crate) async fn list_categories(db: &Database) -> AppResult<CategoriesResponse> {
    let category_documents = repository::category::list_categories(db).await?;
    let subcategory_documents = repository::subcategory::list_all_subcategories(db).await?;

    let mut grouped: HashMap<String, Vec<SubcategoryInfo>> = HashMap::new();
    for document in subcategory_documents {
        grouped
            .entry(document.category_key.clone())
            .or_default()
            .push(document.into_subcategory_info());
    }

    let categories = category_documents
        .into_iter()
        .map(|document| {
            let subcategories = grouped.remove(&document.key).unwrap_or_default();
            document.into_category_info(subcategories)
        })
        .collect();

    Ok(CategoriesResponse { categories })
}

/// Validates required fields and name uniqueness, inserts the category, then
/// inserts one `SubcategoryDocument` per name in `body.subcategories` so a
/// category can be bootstrapped with its starter subcategories in one call.
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

    let now = BsonDateTime::now();
    let category_key = generate_id(prefixes::CATEGORY);
    let document = CategoryDocument {
        id: None,
        key: category_key.clone(),
        name: body.name,
        icon: body.icon,
        color: body.color,
        created_at: now,
        updated_at: now,
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
            created_at: now,
            updated_at: now,
        };
        repository::subcategory::insert_subcategory(db, &subcategory_document).await?;
        subcategories.push(subcategory_document.into_subcategory_info());
    }

    Ok(document.into_category_info(subcategories))
}

/// Updates a category's fields, looked up and persisted by its immutable
/// `key`. Renaming (`name`) is now a pure display-label change — since
/// products/subcategories reference this category by `key`, nothing else
/// needs to be touched.
pub(crate) async fn update_category(
    db: &Database,
    category_key: String,
    body: UpdateCategoryRequest,
) -> AppResult<CategoryInfo> {
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
}

/// Refuses to delete (409 `CATEGORY_IN_USE`) while any product still
/// references this category by `category_key`. Once the guard passes, also
/// cascade-deletes every subcategory under this category — safe because a
/// product can only hold a `subcategory_key` whose owning subcategory has
/// this same `category_key` (enforced by
/// `service::product::ensure_valid_category`), so no product can be left
/// pointing at an orphaned subcategory.
pub(crate) async fn delete_category(
    db: &Database,
    category_key: String,
) -> AppResult<CategoryInfo> {
    let products_using_category =
        repository::product::count_products_in_category(db, &category_key).await?;

    if products_using_category > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::CATEGORY_IN_USE,
            format!(
                "{products_using_category} product(s) still reference category '{category_key}'"
            ),
        ));
    }

    let deleted = repository::category::delete_category(db, &category_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    repository::subcategory::delete_subcategories_by_category(db, &category_key).await?;

    Ok(deleted.into_category_info(Vec::new()))
}

/// Same underlying data as `list_categories`, reshaped as key+name pairs so
/// a client (e.g. a product-creation form) can display names while
/// submitting `categoryKey`/`subcategoryKey`.
pub(crate) async fn get_valid_categories(db: &Database) -> AppResult<ValidCategoriesResponse> {
    let category_documents = repository::category::list_categories(db).await?;
    let subcategory_documents = repository::subcategory::list_all_subcategories(db).await?;

    let mut grouped: HashMap<String, Vec<ValidSubcategoryOption>> = HashMap::new();
    for document in subcategory_documents {
        grouped
            .entry(document.category_key.clone())
            .or_default()
            .push(ValidSubcategoryOption {
                key: document.key,
                name: document.name,
            });
    }

    let categories = category_documents
        .into_iter()
        .map(|document| ValidCategoryOption {
            subcategories: grouped.remove(&document.key).unwrap_or_default(),
            key: document.key,
            name: document.name,
        })
        .collect();

    Ok(ValidCategoriesResponse { categories })
}

/// The subcategories for one category, 404ing if the category itself
/// doesn't exist.
pub(crate) async fn get_category_subcategories(
    db: &Database,
    category_key: String,
) -> AppResult<SubcategoriesResponse> {
    repository::category::find_category_by_key(db, &category_key)
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

    Ok(SubcategoriesResponse {
        category_key,
        subcategories,
    })
}

/// Adds a subcategory under a category, rejecting duplicate names within
/// that same category (409 `SUBCATEGORY_ALREADY_EXISTS`).
pub(crate) async fn add_subcategory(
    db: &Database,
    category_key: String,
    name: String,
) -> AppResult<CategoryInfo> {
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
        created_at: now,
        updated_at: now,
    };
    repository::subcategory::insert_subcategory(db, &subcategory_document).await?;

    let subcategory_documents =
        repository::subcategory::list_subcategories_by_category(db, &category_key).await?;
    let subcategories = subcategory_documents
        .into_iter()
        .map(SubcategoryDocument::into_subcategory_info)
        .collect();

    Ok(category.into_category_info(subcategories))
}

/// Removes a subcategory. Checks existence within this category (404
/// `SUBCATEGORY_NOT_FOUND`) and, like `delete_category`, refuses (409
/// `SUBCATEGORY_IN_USE`) while any product still references this exact
/// `subcategory_key`.
pub(crate) async fn remove_subcategory(
    db: &Database,
    category_key: String,
    subcategory_key: String,
) -> AppResult<CategoryInfo> {
    let category = repository::category::find_category_by_key(db, &category_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    let subcategory = repository::subcategory::find_subcategory_by_key(db, &subcategory_key)
        .await?
        .filter(|document| document.category_key == category_key)
        .ok_or_else(|| {
            AppError::not_found_with_code("Subcategory not found", codes::SUBCATEGORY_NOT_FOUND)
        })?;

    let products_using_subcategory =
        repository::product::count_products_in_subcategory(db, &subcategory_key).await?;

    if products_using_subcategory > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SUBCATEGORY_IN_USE,
            format!(
                "{products_using_subcategory} product(s) still reference subcategory '{}'",
                subcategory.name
            ),
        ));
    }

    repository::subcategory::delete_subcategory_by_key(db, &subcategory_key).await?;

    let subcategory_documents =
        repository::subcategory::list_subcategories_by_category(db, &category_key).await?;
    let subcategories = subcategory_documents
        .into_iter()
        .map(SubcategoryDocument::into_subcategory_info)
        .collect();

    Ok(category.into_category_info(subcategories))
}
