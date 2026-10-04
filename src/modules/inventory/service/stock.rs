// Business rules for stock changes: the adjustment invariant (never let
// stock go negative), and the fact that every adjustment must also produce
// a `StockMovement` audit record — the two collections are always written
// together from here, never independently.

use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        LowStockItem, LowStockResponse, StockAdjustmentRequest, StockAdjustmentResponse,
        StockMovementListQuery, StockMovementListResponse, StockMovementType,
        StockMovementsResponse,
    },
    modules::inventory::{model::ProductDocument, model::StockMovementDocument, repository},
};

/// Applies a signed `delta` to a product's stock and records the change as
/// a `StockMovement` in the same write — the two collections are always
/// written together, never independently (see the module banner). Rejects
/// with `INSUFFICIENT_STOCK` rather than clamping to zero, since a caller
/// requesting a delta larger than available stock is almost always a bug
/// (e.g. double-submitted sale) that should surface as an error, not
/// silently produce a wrong quantity. Shared by `adjust_stock` (manual,
/// tagged `ManualAdjustment`) and, cross-module, by
/// `purchases::service::purchase::record_purchase` (tagged
/// `PurchaseReceipt`, `reference_id` = the purchase's key) — the one place
/// stock is ever mutated, so every caller gets the same guard and audit
/// trail for free.
pub(crate) async fn apply_stock_delta(
    db: &Db,
    id: ObjectId,
    delta: i64,
    movement_type: StockMovementType,
    reference_id: Option<String>,
    note: Option<String>,
) -> AppResult<(i64, ProductDocument)> {
    let now = BsonDateTime::now();
    let (previous_stock_quantity, updated) =
        match repository::product::adjust_product_stock_pipeline(db, id, delta).await? {
            Some(res) => res,
            None => {
                let existing = repository::product::find_product_by_id(db, id)
                    .await?
                    .ok_or_else(|| {
                        AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
                    })?;
                let requested_qty = -delta;
                let message = format!(
                    "Only {} unit{} of {} {} left in stock.",
                    existing.stock_quantity,
                    if existing.stock_quantity == 1 {
                        ""
                    } else {
                        "s"
                    },
                    existing.name,
                    if existing.stock_quantity == 1 {
                        "is"
                    } else {
                        "are"
                    }
                );
                return Err(AppError::conflict_with_details(
                    codes::INSUFFICIENT_STOCK,
                    message,
                    serde_json::json!({
                        "available": existing.stock_quantity,
                        "requested": requested_qty,
                        "productId": existing.id.map(|i| i.to_hex()).unwrap_or_default(),
                    }),
                ));
            }
        };

    repository::stock::insert_stock_movement(
        db,
        StockMovementDocument {
            id: None,
            key: generate_id(prefixes::STOCK_MOVEMENT),
            product_id: id,
            quantity_delta: delta,
            movement_type,
            reference_id,
            note,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        },
    )
    .await?;

    Ok((previous_stock_quantity, updated))
}

/// Manual stock adjustment via `PATCH /products/{id}/stock` — thin wrapper
/// around `apply_stock_delta` tagging the movement `ManualAdjustment`, then
/// reshaping the result into the API response (which also echoes back the
/// pre-adjustment quantity so a client can display the change without a
/// second request).
pub(crate) async fn adjust_stock(
    db: &Db,
    id: ObjectId,
    body: StockAdjustmentRequest,
) -> AppResult<StockAdjustmentResponse> {
    crate::core::logging::domain::tracked("inventory.stock_adjusted", async move {
        let (previous_stock_quantity, updated) = apply_stock_delta(
            db,
            id,
            body.delta,
            StockMovementType::ManualAdjustment,
            None,
            body.reason,
        )
        .await?;

        Ok(StockAdjustmentResponse {
            id: updated
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key: updated.key,
            sku: updated.sku,
            name: updated.name,
            stock_quantity: updated.stock_quantity,
            previous_stock_quantity,
            delta: body.delta,
            updated_at: updated.updated_at.to_chrono(),
        })
    })
    .await
}

/// Products at or below their configured reorder threshold.
pub(crate) async fn low_stock(db: &Db) -> AppResult<LowStockResponse> {
    let documents = repository::product::find_low_stock_products(db).await?;

    let items: Vec<LowStockItem> = documents
        .into_iter()
        .map(|document| LowStockItem {
            id: document
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key: document.key,
            sku: document.sku,
            name: document.name,
            stock_quantity: document.stock_quantity,
            min_stock_threshold: document.min_stock_threshold,
            deficit: document.min_stock_threshold - document.stock_quantity,
        })
        .collect();

    let total = items.len() as u64;

    Ok(LowStockResponse { items, total })
}

/// The audit-trail history for one product. 404s if the product itself
/// doesn't exist (rather than just returning an empty list), so a typo'd
/// id is distinguishable from a real product with no movements yet.
pub(crate) async fn product_movements(db: &Db, id: ObjectId) -> AppResult<StockMovementsResponse> {
    if repository::product::find_product_by_id(db, id)
        .await?
        .is_none()
    {
        return Err(AppError::not_found_with_code(
            "Product not found",
            codes::PRODUCT_NOT_FOUND,
        ));
    }

    let documents = repository::stock::find_stock_movements_for_product(db, id).await?;
    let movements = documents
        .into_iter()
        .map(StockMovementDocument::into_stock_movement)
        .collect();

    Ok(StockMovementsResponse { movements })
}

/// Unfiltered, paginated listing of all stock movements for delta/offline sync.
pub(crate) async fn list_stock_movements(
    db: &Db,
    query: StockMovementListQuery,
) -> AppResult<StockMovementListResponse> {
    let mut filter = doc! { "deleted_at": { "$exists": false } };

    if let Some(ref pid_str) = query.product_id
        && !pid_str.is_empty()
    {
        let pid =
            ObjectId::parse_str(pid_str).map_err(|_| AppError::validation("Invalid product ID"))?;
        filter.insert("product_id", pid);
    }

    if let Some(mtype) = query.movement_type {
        let mtype_str = match mtype {
            StockMovementType::Sale => "sale",
            StockMovementType::PurchaseReceipt => "purchase_receipt",
            StockMovementType::RepairPartConsumption => "repair_part_consumption",
            StockMovementType::ManualAdjustment => "manual_adjustment",
            StockMovementType::InvoiceVoidReversal => "invoice_void_reversal",
            StockMovementType::ReturnRestock => "return_restock",
            StockMovementType::ReturnWriteOff => "return_write_off",
            StockMovementType::ReturnSupplierRma => "return_supplier_rma",
            StockMovementType::OpeningBalance => "opening_balance",
        };
        filter.insert("movement_type", mtype_str);
    }

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 500);
    let skip = (page - 1) * limit;

    let (documents, total) =
        repository::stock::list_stock_movements_paginated(db, filter, skip, limit as i64).await?;

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };

    let items = documents
        .into_iter()
        .map(StockMovementDocument::into_stock_movement)
        .collect();

    Ok(StockMovementListResponse {
        items,
        pagination: crate::domain::inventory::PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        },
    })
}

/// Converts a page of raw `stock_movements` documents — as read by the sync
/// module's cursor scan — into the `StockMovement` shape the REST reads
/// return. Needs no lookups: a movement carries everything it exposes.
/// See `service::product::hydrate_sync_documents` for why the two feeds
/// must agree.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<crate::domain::inventory::StockMovement>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<StockMovementDocument>(document)?
                    .into_stock_movement(),
            )
        })
        .collect()
}
