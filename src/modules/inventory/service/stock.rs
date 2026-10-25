// Business rules for stock changes: the adjustment invariant (never let a
// batch's or a product's stock go negative), and the fact that every
// adjustment must also produce a `StockMovement` audit record — the batch,
// the product total and the movement are always written together from here,
// never independently.

use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};

use crate::{
    clients::{db::Db, uow::Unit},
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        BatchSource, LowStockItem, LowStockResponse, StockAdjustmentRequest,
        StockAdjustmentResponse, StockMovementListQuery, StockMovementListResponse,
        StockMovementType, StockMovementsResponse,
    },
    modules::inventory::{model::ProductDocument, model::StockMovementDocument, repository},
};

/// Applies a signed `delta` to one batch of a product and to the product's
/// total, and records the change as a `StockMovement` carrying the batch key
/// — the three are always written together, never independently (see the
/// module banner). Every unit of stock lives in a batch, so a quantity change
/// always names one; only the audit-only return movements (`delta == 0`)
/// carry no batch. Rejects with `INSUFFICIENT_STOCK` rather than clamping to
/// zero, since a delta larger than what the batch holds is almost always a
/// bug (e.g. a double-submitted sale) that should surface as an error. The
/// batch is moved first because its guard is the stricter one; if the
/// product write then fails, the batch move is undone so the two never
/// disagree. This is the one place stock is ever mutated, so every caller
/// gets the same guard and audit trail for free.
pub(crate) async fn apply_stock_delta(
    unit: impl Into<Unit<'_>>,
    id: ObjectId,
    batch_key: Option<&str>,
    delta: i64,
    movement_type: StockMovementType,
    reference_id: Option<String>,
    note: Option<String>,
) -> AppResult<(i64, ProductDocument)> {
    let mut unit = unit.into();
    if delta != 0 && batch_key.is_none() {
        return Err(AppError::internal(
            "A stock quantity change must name the batch it moves",
        ));
    }

    if let Some(batch_key) = batch_key
        && delta != 0
        && repository::stock_batch::adjust_batch_remaining(&mut unit, batch_key, delta)
            .await?
            .is_none()
    {
        let batch = repository::stock_batch::find_batch_by_key(&mut unit, batch_key)
            .await?
            .ok_or_else(|| super::stock_batch::batch_not_found(batch_key))?;
        let name = repository::product::find_product_by_id(&mut unit, id)
            .await?
            .map(|product| product.name)
            .unwrap_or_default();
        return Err(insufficient_stock(
            batch.remaining_quantity,
            -delta,
            &name,
            serde_json::json!({
                "available": batch.remaining_quantity,
                "requested": -delta,
                "productId": id.to_hex(),
                "batchKey": batch_key,
                "sellingPriceCents": batch.selling_price_cents,
            }),
        ));
    }

    let adjusted = repository::product::adjust_product_stock_pipeline(&mut unit, id, delta).await;
    let (previous_stock_quantity, updated) = match adjusted {
        Ok(Some(result)) => result,
        failed => {
            if let Some(batch_key) = batch_key
                && delta != 0
            {
                repository::stock_batch::adjust_batch_remaining(&mut unit, batch_key, -delta).await?;
            }
            failed?;
            let existing = repository::product::find_product_by_id(&mut unit, id)
                .await?
                .ok_or_else(|| {
                    AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
                })?;
            return Err(insufficient_stock(
                existing.stock_quantity,
                -delta,
                &existing.name,
                serde_json::json!({
                    "available": existing.stock_quantity,
                    "requested": -delta,
                    "productId": id.to_hex(),
                }),
            ));
        }
    };

    let now = BsonDateTime::now();
    repository::stock::insert_stock_movement(
        &mut unit,
        StockMovementDocument {
            id: None,
            key: generate_id(prefixes::STOCK_MOVEMENT),
            product_id: id,
            batch_key: batch_key.map(str::to_string),
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

fn insufficient_stock(
    available: i64,
    requested: i64,
    name: &str,
    details: serde_json::Value,
) -> AppError {
    let message = format!(
        "Only {available} unit{} of {name} {} left in stock (asked for {requested}).",
        if available == 1 { "" } else { "s" },
        if available == 1 { "is" } else { "are" },
    );
    AppError::conflict_with_details(codes::INSUFFICIENT_STOCK, message, details)
}

/// Manual stock adjustment via `PATCH /products/{id}/stock`. A negative
/// delta takes units out of the named batch; a positive delta brings units
/// in as a new `adjustment` batch with its own cost and selling price (stock
/// never exists outside a batch). Echoes back the pre-adjustment quantity so
/// a client can display the change without a second request.
pub(crate) async fn adjust_stock(
    db: &Db,
    id: ObjectId,
    body: StockAdjustmentRequest,
) -> AppResult<StockAdjustmentResponse> {
    crate::core::logging::domain::tracked("inventory.stock_adjusted", async move {
        let product = repository::product::find_product_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;

        let (previous_stock_quantity, updated) = match body.delta {
            0 => return Err(AppError::validation("Adjustment delta cannot be 0")),
            delta if delta < 0 => {
                let batch_key = body.batch_key.as_deref().ok_or_else(|| {
                    AppError::validation("Taking stock out needs the batchKey it comes from")
                })?;
                let batch = repository::stock_batch::find_batch_by_key(db, batch_key)
                    .await?
                    .filter(|batch| batch.product_key == product.key)
                    .ok_or_else(|| super::stock_batch::batch_not_found(batch_key))?;
                apply_stock_delta(
                    db,
                    id,
                    Some(&batch.key),
                    delta,
                    StockMovementType::ManualAdjustment,
                    None,
                    body.reason,
                )
                .await?
            }
            delta => {
                let (Some(unit_cost_cents), Some(selling_price_cents)) =
                    (body.unit_cost_cents, body.selling_price_cents)
                else {
                    return Err(AppError::validation(
                        "Adding stock needs the unitCostCents and sellingPriceCents of the new units",
                    ));
                };
                let previous = product.stock_quantity;
                super::stock_batch::create_batch(
                    db,
                    super::stock_batch::NewBatch {
                        product_id: id,
                        product_key: product.key.clone(),
                        source: BatchSource::Adjustment,
                        purchase_key: None,
                        supplier_key: None,
                        quantity: delta,
                        unit_cost_cents,
                        selling_price_cents,
                        received_at: BsonDateTime::now(),
                        movement_type: StockMovementType::ManualAdjustment,
                        reference_id: None,
                        note: body.reason,
                    },
                )
                .await?;
                let updated = repository::product::find_product_by_id(db, id)
                    .await?
                    .expect("product was just adjusted and must exist");
                (previous, updated)
            }
        };

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
