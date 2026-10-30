use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::PaginationMeta;
use crate::domain::supplier_products::{
    SupplierProductLink, SupplierProductLinkQuery, SupplierProductListResponse,
};

use crate::modules::supplier_products::model::SupplierProductLinkDocument;
use crate::modules::supplier_products::repository;

use mongodb::bson::doc;

/// Lists links scoped to a supplier, a product, their intersection, or all links paginated.
pub async fn list_links(
    db: &Db,
    query: SupplierProductLinkQuery,
) -> AppResult<SupplierProductListResponse> {
    let mut filter = doc! { "deleted_at": { "$exists": false } };

    if let Some(ref supplier_key) = query.supplier_key {
        filter.insert("supplier_key", supplier_key);
    }
    if let Some(ref product_key) = query.product_key {
        filter.insert("product_key", product_key);
    }

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 500);
    let skip = (page - 1) * limit;

    let (documents, total) =
        repository::list_links_paginated(db, filter, skip, limit as i64).await?;

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };

    let items: Vec<SupplierProductLink> = documents
        .into_iter()
        .map(SupplierProductLinkDocument::into_link)
        .collect();

    Ok(SupplierProductListResponse {
        links: items.clone(),
        items,
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        },
    })
}
