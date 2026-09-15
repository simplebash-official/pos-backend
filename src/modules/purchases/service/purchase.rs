// Business rules for recording and listing supplier purchases: the
// quantity/cost invariants, resolving both sides of a purchase (supplier,
// product) before it's ever written, and the fact that recording a
// purchase always also bumps the product's stock and writes a
// `PurchaseReceipt` movement — those two writes never happen independently
// (see `inventory::service::stock::apply_stock_delta`, which this reuses).

use std::collections::HashMap;

use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        constants::prefixes,
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        inventory::{PaginationMeta, StockMovementType},
        purchases::{
            CreatePurchaseRequest, ProductSummary, Purchase, PurchaseListQuery,
            PurchaseListResponse, SupplierSummary,
        },
    },
    modules::{
        inventory::service::{
            product as inventory_product, product_serial as inventory_product_serial,
            stock as inventory_stock,
        },
        purchases::{model::PurchaseDocument, repository},
        suppliers::service as supplier_service,
    },
};

/// Validates the purchase, resolves the supplier/product it references,
/// records it, then applies the stock increment + `PurchaseReceipt`
/// movement in the same call inventory's own manual adjustment uses —
/// reusing the one place stock is ever mutated rather than duplicating that
/// invariant here.
pub async fn record_purchase(db: &Db, body: CreatePurchaseRequest) -> AppResult<Purchase> {
    crate::core::logging::domain::tracked("purchases.recorded", async move {
        if body.quantity < 1 {
            return Err(AppError::validation("Quantity must be at least 1"));
        }
        if body.unit_cost_cents < 0 {
            return Err(AppError::validation("Unit cost cannot be negative"));
        }

        let supplier = supplier_service::get_supplier_by_key(db, &body.supplier_key).await?;
        let product = inventory_product::get_product_by_key(db, &body.product_key).await?;

        let serial_numbers = body.serial_numbers.clone().unwrap_or_default();
        if product.is_serialized && serial_numbers.len() as i64 != body.quantity {
            return Err(AppError::validation(format!(
                "This product is serialized — provide exactly {} serial number(s), got {}",
                body.quantity,
                serial_numbers.len()
            )));
        }

        let now = BsonDateTime::now();
        let key = generate_id(prefixes::PURCHASE);
        let document = PurchaseDocument {
            id: None,
            key: key.clone(),
            supplier_key: body.supplier_key,
            product_key: body.product_key,
            quantity: body.quantity,
            unit_cost_cents: body.unit_cost_cents,
            date: BsonDateTime::from_chrono(body.date),
            reference_no: body.reference_no.clone(),
            notes: body.notes,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };

        let inserted = repository::insert_purchase(db, document).await?;

        let product_object_id = ObjectId::parse_str(&product.id).expect(
            "Product.id returned from get_product_by_key is always a valid ObjectId hex string",
        );
        inventory_stock::apply_stock_delta(
            db,
            product_object_id,
            body.quantity,
            StockMovementType::PurchaseReceipt,
            Some(key),
            body.reference_no,
        )
        .await?;

        if product.is_serialized {
            inventory_product_serial::create_serials_for_purchase(
                db,
                &product.key,
                &serial_numbers,
            )
            .await?;
        }

        let supplier_summary = SupplierSummary {
            id: supplier.id,
            key: supplier.key,
            name: supplier.name,
            contact_person: supplier.contact_person,
            primary_phone: supplier.primary_phone,
        };
        let product_summary = ProductSummary {
            id: product.id,
            key: product.key,
            sku: product.sku,
            name: product.name,
            category: product.category,
            subcategory: product.subcategory,
        };

        Ok(inserted.into_purchase(Some(supplier_summary), Some(product_summary)))
    })
    .await
}

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

    let items: Vec<Purchase> = documents
        .into_iter()
        .map(|document| {
            let supplier = suppliers_by_key.get(&document.supplier_key).cloned();
            let product = products_by_key.get(&document.product_key).cloned();
            document.into_purchase(supplier, product)
        })
        .collect();

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

/// Used by `suppliers::service::delete_supplier`'s `SUPPLIER_HAS_PURCHASES`
/// guard.
pub(crate) async fn count_purchases_for_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    repository::count_purchases_for_supplier(db, supplier_key).await
}

/// Converts a page of raw `purchases` documents — as read by the sync
/// module's cursor scan — into the `Purchase` shape `GET /purchases`
/// returns, embedded supplier/product summaries included. The summaries are
/// resolved with the same batch lookups `list_purchases` uses, so a row that
/// arrives by delta carries the same display data as one from a snapshot.
/// See `inventory::service::product::hydrate_sync_documents`.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<Purchase>> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let purchases: Vec<PurchaseDocument> = documents
        .into_iter()
        .map(|document| {
            bson::deserialize_from_document::<PurchaseDocument>(document).map_err(AppError::from)
        })
        .collect::<AppResult<Vec<_>>>()?;

    let supplier_keys: Vec<String> = purchases
        .iter()
        .map(|purchase| purchase.supplier_key.clone())
        .collect();
    let product_keys: Vec<String> = purchases
        .iter()
        .map(|purchase| purchase.product_key.clone())
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

    Ok(purchases
        .into_iter()
        .map(|purchase| {
            let supplier = suppliers_by_key.get(&purchase.supplier_key).cloned();
            let product = products_by_key.get(&purchase.product_key).cloned();
            purchase.into_purchase(supplier, product)
        })
        .collect())
}
