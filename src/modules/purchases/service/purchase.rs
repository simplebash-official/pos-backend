// Business rules for reading supplier purchase lines. A line is created only as part of a
// receipt (`super::receipt`), which resolves both sides (supplier, product) before anything is
// written and brings the units in as a new stock batch. This file lists and enriches lines.

use std::collections::HashMap;

use mongodb::bson::doc;

use crate::{
    clients::db::Db,
    core::error::{AppError, AppResult},
    domain::{
        inventory::PaginationMeta,
        purchases::{
            ProductSummary, Purchase, PurchaseListQuery, PurchaseListResponse, SupplierSummary,
        },
    },
    modules::{
        inventory::service::product as inventory_product,
        purchases::{model::PurchaseDocument, repository},
        suppliers::service as supplier_service,
    },
};

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

/// Attaches the live supplier and product summaries to purchase lines, with the same two batch
/// lookups for every caller (list, receipt, sync), so a line carries the same display data
/// however it is delivered. A reference that no longer resolves is left off, never faked.
pub(crate) async fn enrich_purchases(
    db: &Db,
    documents: Vec<PurchaseDocument>,
) -> AppResult<Vec<Purchase>> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let supplier_keys: Vec<String> = documents
        .iter()
        .map(|document| document.supplier_key.clone())
        .collect();
    let product_keys: Vec<String> = documents
        .iter()
        .map(|document| document.product_key.clone())
        .collect();

    let suppliers_by_key: HashMap<String, SupplierSummary> =
        supplier_service::get_suppliers_by_keys(db, &supplier_keys)
            .await?
            .into_iter()
            .map(|supplier| {
                (
                    supplier.key.clone(),
                    SupplierSummary {
                        id: supplier.id,
                        key: supplier.key,
                        name: supplier.name,
                        contact_person: supplier.contact_person,
                        primary_phone: supplier.primary_phone,
                    },
                )
            })
            .collect();
    let products_by_key: HashMap<String, ProductSummary> =
        inventory_product::get_products_by_keys(db, &product_keys)
            .await?
            .into_iter()
            .map(|product| {
                (
                    product.key.clone(),
                    ProductSummary {
                        id: product.id,
                        key: product.key,
                        sku: product.sku,
                        name: product.name,
                        category: product.category,
                        subcategory: product.subcategory,
                    },
                )
            })
            .collect();

    Ok(documents
        .into_iter()
        .map(|document| {
            let supplier = suppliers_by_key.get(&document.supplier_key).cloned();
            let product = products_by_key.get(&document.product_key).cloned();
            document.into_purchase(supplier, product)
        })
        .collect())
}

/// What was bought from a supplier, as `(YYYY-MM-DD, total cost in cents)` per purchase.
pub(crate) async fn costs_of_supplier(db: &Db, supplier_key: &str) -> AppResult<Vec<(String, i64)>> {
    Ok(repository::list_purchases_by_supplier(db, supplier_key)
        .await?
        .into_iter()
        .map(|p| (p.date.to_chrono().format("%Y-%m-%d").to_string(), p.quantity * p.unit_cost_cents))
        .collect())
}

/// Used by `suppliers::service::delete_supplier`'s `SUPPLIER_HAS_PURCHASES`
/// guard.
pub(crate) async fn count_purchases_for_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    repository::count_purchases_for_supplier(db, supplier_key).await
}

/// Converts a page of raw `purchases` documents — as read by the sync
/// module's cursor scan — into the `Purchase` shape `GET /purchases`
/// returns, embedded supplier/product summaries included, so a row that
/// arrives by delta carries the same display data as one from a snapshot.
/// See `inventory::service::product::hydrate_sync_documents`.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<Purchase>> {
    let purchases: Vec<PurchaseDocument> = documents
        .into_iter()
        .map(|document| {
            bson::deserialize_from_document::<PurchaseDocument>(document).map_err(AppError::from)
        })
        .collect::<AppResult<Vec<_>>>()?;
    enrich_purchases(db, purchases).await
}
