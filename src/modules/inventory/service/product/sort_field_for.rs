use crate::modules::inventory::repository;

/// Maps a client-supplied `sortBy` value to the corresponding document field.
/// Whitelisted rather than passed straight through, since sort keys end up
/// directly in a Mongo `sort` document.
pub(crate) fn sort_field_for(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("name") => "name",
        Some("sku") => "sku",
        Some("category") => "category_key",
        Some("subcategory") => "subcategory_key",
        // Computed per row from the in-stock batches (see
        // `repository::product::list_products_with_display_names`).
        Some("sellingPriceCents") => repository::product::MIN_SELLING_PRICE,
        Some("stockQuantity") => "stock_quantity",
        Some("minStockThreshold") => "min_stock_threshold",
        Some("createdAt") => "created_at",
        Some("updatedAt") => "updated_at",
        _ => "updated_at",
    }
}
