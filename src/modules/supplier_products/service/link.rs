// Business rules for supplier-product linking: validating that both sides
// of a link actually exist (delegating to `inventory::service::product` and
// `suppliers::service` — the two collections this module joins), the
// upsert-not-duplicate behavior on create, and the two cascade-delete entry
// points other modules call when a supplier or product is removed.

use std::collections::HashMap;

use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, doc},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        inventory::PaginationMeta,
        supplier_products::{
            BulkReplaceLinksRequest, SupplierProductLink, SupplierProductLinkQuery,
            SupplierProductListResponse, UpsertSupplierProductLinkRequest,
        },
    },
    modules::{
        inventory::service::product as inventory_product,
        supplier_products::{model::SupplierProductLinkDocument, repository},
        suppliers::service as supplier_service,
    },
};

/// Lists links scoped to a supplier, a product, their intersection, or all links paginated.
pub async fn list_links(
    db: &Database,
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

/// Validates both `supplierKey` and `productKey` resolve to real documents,
/// then creates the link or, if `(supplierKey, productKey)` already has one,
/// updates its `costPriceCents`/`notes` in place rather than erroring on a
/// duplicate (matching the frontend's upsert contract for this endpoint).
pub async fn upsert_link(
    db: &Database,
    body: UpsertSupplierProductLinkRequest,
) -> AppResult<SupplierProductLink> {
    if let Some(cost_price_cents) = body.cost_price_cents
        && cost_price_cents < 0
    {
        return Err(AppError::validation("Cost price cannot be negative"));
    }

    supplier_service::get_supplier_by_key(db, &body.supplier_key).await?;
    inventory_product::get_product_by_key(db, &body.product_key).await?;

    let existing = repository::find_link(db, &body.supplier_key, &body.product_key).await?;

    let document = match existing {
        Some(_) => {
            let mut set_doc = doc! { "updated_at": BsonDateTime::now() };
            set_doc.insert("cost_price_cents", body.cost_price_cents);
            set_doc.insert("notes", body.notes);
            // `update_link` owns the `version` bump and the `deleted_at`
            // tombstone clear, matching `suppliers::repository::update_supplier`.
            repository::update_link(db, &body.supplier_key, &body.product_key, set_doc)
                .await?
                .expect("link existed moments ago, update_link must find it")
        }
        None => {
            let now = BsonDateTime::now();
            let document = SupplierProductLinkDocument {
                id: None,
                key: generate_id(prefixes::SUPPLIER_PRODUCT),
                supplier_key: body.supplier_key,
                product_key: body.product_key,
                cost_price_cents: body.cost_price_cents,
                notes: body.notes,
                version: 1,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                updated_by_device: None,
            };
            repository::insert_link(db, document).await?
        }
    };

    Ok(document.into_link())
}

/// Unlinks a supplier from a product, 404ing with
/// `SUPPLIER_PRODUCT_LINK_NOT_FOUND` if no such link exists.
pub(crate) async fn delete_link(
    db: &Database,
    supplier_key: &str,
    product_key: &str,
) -> AppResult<()> {
    repository::delete_link(db, supplier_key, product_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                "Supplier-product link not found",
                codes::SUPPLIER_PRODUCT_LINK_NOT_FOUND,
            )
        })?;
    Ok(())
}

/// Replaces every link for a supplier with the given set of `productKeys`:
/// validates the supplier and every product key exist, then deletes all of
/// the supplier's existing links and re-inserts the new set — preserving
/// `costPriceCents`/`notes` for any product key that already had a link.
pub(crate) async fn replace_links_for_supplier(
    db: &Database,
    supplier_key: String,
    body: BulkReplaceLinksRequest,
) -> AppResult<Vec<SupplierProductLink>> {
    supplier_service::get_supplier_by_key(db, &supplier_key).await?;
    for product_key in &body.product_keys {
        inventory_product::get_product_by_key(db, product_key).await?;
    }

    let existing_by_product: HashMap<String, SupplierProductLinkDocument> =
        repository::list_links_by_supplier(db, &supplier_key)
            .await?
            .into_iter()
            .map(|link| (link.product_key.clone(), link))
            .collect();

    repository::delete_links_by_supplier(db, &supplier_key).await?;

    let now = BsonDateTime::now();
    let mut links = Vec::with_capacity(body.product_keys.len());
    for product_key in body.product_keys {
        let document = match existing_by_product.get(&product_key) {
            Some(existing) => SupplierProductLinkDocument {
                id: None,
                key: existing.key.clone(),
                supplier_key: supplier_key.clone(),
                product_key,
                cost_price_cents: existing.cost_price_cents,
                notes: existing.notes.clone(),
                version: existing.version + 1,
                created_at: existing.created_at,
                updated_at: now,
                deleted_at: None,
                updated_by_device: None,
            },
            None => SupplierProductLinkDocument {
                id: None,
                key: generate_id(prefixes::SUPPLIER_PRODUCT),
                supplier_key: supplier_key.clone(),
                product_key,
                cost_price_cents: None,
                notes: None,
                version: 1,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                updated_by_device: None,
            },
        };
        let inserted = repository::insert_link(db, document).await?;
        links.push(inserted.into_link());
    }

    Ok(links)
}

/// Cascade entry point called from `suppliers::service::delete_supplier`.
pub(crate) async fn delete_links_for_supplier(db: &Database, supplier_key: &str) -> AppResult<u64> {
    repository::delete_links_by_supplier(db, supplier_key).await
}

/// Cascade entry point called from
/// `inventory::service::product::delete_product`/`delete_products`.
pub(crate) async fn delete_links_for_product(db: &Database, product_key: &str) -> AppResult<u64> {
    repository::delete_links_by_product(db, product_key).await
}

/// Converts a page of raw `supplier_products` documents — as read by the
/// sync module's cursor scan — into the `SupplierProductLink` shape the
/// REST reads return. See
/// `inventory::service::product::hydrate_sync_documents` for why the delta
/// and snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<SupplierProductLink>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<SupplierProductLinkDocument>(document)?
                    .into_link(),
            )
        })
        .collect()
}
