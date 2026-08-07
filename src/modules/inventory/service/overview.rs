// Orchestration for the hierarchical Inventory & Stock overview endpoint.
// Combines metrics aggregation, category & subcategory tree building,
// free-text search across products/categories/subcategories, low-stock filter,
// and per-subcategory product data table pagination.

use mongodb::{
    Database,
    bson::{Document, doc},
};

use crate::{
    core::{
        error::AppResult,
        utils::{build_bson_regex, calculate_pagination},
    },
    domain::inventory::{
        InventoryCategoryOverview, InventoryMetrics, InventoryOverviewQuery,
        InventoryOverviewResponse, InventorySubcategoryOverview, PaginationMeta,
    },
    modules::inventory::repository,
};

/// Computes the complete hierarchical overview for the Inventory & Stock frontend page.
/// Performs MongoDB aggregation pipelines to calculate dashboard metrics and group item counts.
pub async fn get_inventory_overview(
    db: &Database,
    query: InventoryOverviewQuery,
) -> AppResult<InventoryOverviewResponse> {
    // 1. Fetch every category paired with its subcategories in one `$lookup`
    // round trip — always shown in full regardless of the current filter (a
    // category/subcategory with zero matching products still renders, just
    // with an empty table). `total_categories`/`total_subcategories` are
    // derived from this same result rather than two extra `count_documents`
    // calls.
    let categories_with_subcategories =
        repository::category::list_categories_with_subcategories(db).await?;
    let total_categories = categories_with_subcategories.len() as u64;
    let total_subcategories = categories_with_subcategories
        .iter()
        .map(|(_, subcategories)| subcategories.len() as u64)
        .sum();

    // 3. Build Mongo filter for matching products
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(category_key) = query.category_key.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "category_key": category_key });
    }
    if let Some(subcategory_key) = query.subcategory_key.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "subcategory_key": subcategory_key });
    }
    if query.low_stock == Some(true) {
        and_clauses.push(doc! {
            "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
        });
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

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 10, 100);
    let sort_field = super::product::sort_field_for(query.sort_by.as_deref());
    let sort_order = if query.sort_order.as_deref() == Some("asc") {
        1
    } else {
        -1
    };

    // 4. One aggregation round-trip computes everything product-side: the
    // unfiltered dashboard metrics, filtered per-category counts, and
    // filtered+sorted+paginated per-subcategory product pages — replacing
    // what used to be a separate query per subcategory.
    let overview = repository::product::aggregate_overview(
        db,
        filter,
        doc! { sort_field: sort_order },
        skip,
        limit as i64,
    )
    .await?;

    let metrics = InventoryMetrics {
        total_items: overview.total_items,
        total_categories,
        total_subcategories,
        low_stock_alerts: overview.low_stock_alerts,
    };

    // 5. Build category & subcategory hierarchy from the reference data
    // fetched in step 1, attaching each subcategory's page of products
    // straight from the aggregation result — no further DB calls here.
    let mut subcategory_data = overview.subcategory_data;
    let mut category_overviews = Vec::new();

    for (cat, cat_subcats) in categories_with_subcategories {
        let subcategories_count = cat_subcats.len() as u64;
        let category_total_items = overview.category_counts.get(&cat.key).copied().unwrap_or(0);

        let mut subcat_overviews = Vec::new();

        for subcat in cat_subcats {
            let (total_items, products) = subcategory_data.remove(&subcat.key).unwrap_or_default();
            let products = products
                .into_iter()
                .map(|document| document.into_product(cat.name.clone(), subcat.name.clone()))
                .collect();

            let pagination = PaginationMeta {
                page,
                limit,
                total: total_items,
                total_pages: if limit > 0 {
                    total_items.div_ceil(limit)
                } else {
                    0
                },
            };

            subcat_overviews.push(InventorySubcategoryOverview {
                key: subcat.key,
                category_key: subcat.category_key,
                name: subcat.name,
                total_items,
                products,
                pagination,
            });
        }

        category_overviews.push(InventoryCategoryOverview {
            key: cat.key,
            name: cat.name,
            icon: cat.icon,
            color: cat.color,
            total_items: category_total_items,
            subcategories_count,
            subcategories: subcat_overviews,
        });
    }

    Ok(InventoryOverviewResponse {
        metrics,
        categories: category_overviews,
    })
}
