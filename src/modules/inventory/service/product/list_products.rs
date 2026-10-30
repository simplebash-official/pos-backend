use super::list_sort_for::list_sort_for;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::utils::{build_bson_regex, calculate_pagination};
use crate::domain::inventory::{PaginationMeta, ProductListQuery, ProductListResponse};

use crate::modules::inventory::repository;

use mongodb::bson::{Document, doc};

/// Builds the Mongo filter/sort from query params (category, subcategory,
/// free-text search across name/sku/barcode/category/subcategory, and the low-stock flag) and
/// delegates execution + pagination math to `repository::product::list_products_with_display_names`.
/// Joining categories and subcategories happens inside MongoDB via `$lookup` in a single query.
pub async fn list_products(db: &Db, query: ProductListQuery) -> AppResult<ProductListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(category_key) = query.category_key.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "category_key": category_key });
    }
    if let Some(subcategory_key) = query.subcategory_key.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "subcategory_key": subcategory_key });
    }
    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        let matched_cat_keys =
            repository::category::find_category_keys_by_name_pattern(db, &pattern).await?;
        let matched_subcat_keys =
            repository::subcategory::find_subcategory_keys_by_name_pattern(db, &pattern).await?;

        let mut or_clauses = vec![
            doc! { "name": { "$regex": pattern.clone() } },
            doc! { "sku": { "$regex": pattern.clone() } },
            doc! { "barcode": { "$regex": pattern } },
        ];
        if !matched_cat_keys.is_empty() {
            or_clauses.push(doc! { "category_key": { "$in": matched_cat_keys } });
        }
        if !matched_subcat_keys.is_empty() {
            or_clauses.push(doc! { "subcategory_key": { "$in": matched_subcat_keys } });
        }
        and_clauses.push(doc! { "$or": or_clauses });
    }
    if query.low_stock == Some(true) {
        and_clauses.push(doc! {
            "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
        });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);

    let sort = list_sort_for(query.sort_by.as_deref(), query.sort_order.as_deref());

    let (mut items, total) =
        repository::product::list_products_with_display_names(db, filter, sort, skip, limit as i64)
            .await?;
    super::super::stock_batch::attach_price_tiers(db, &mut items).await?;

    let pagination = PaginationMeta {
        page,
        limit,
        total,
        total_pages: total.div_ceil(limit),
    };

    Ok(ProductListResponse { items, pagination })
}
