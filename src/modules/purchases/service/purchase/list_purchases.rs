use super::enrich_purchases::enrich_purchases;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::inventory::PaginationMeta;
use crate::domain::purchases::{PurchaseListQuery, PurchaseListResponse};

use crate::modules::purchases::repository;

use mongodb::bson::doc;

/// Lists purchases scoped to a supplier, a product, their intersection, or all purchases paginated.
pub async fn list_purchases(db: &Db, query: PurchaseListQuery) -> AppResult<PurchaseListResponse> {
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
        repository::list_purchases_paginated(db, filter, skip, limit as i64).await?;

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };

    let items = enrich_purchases(db, documents).await?;

    Ok(PurchaseListResponse {
        purchases: items.clone(),
        items,
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        },
    })
}
