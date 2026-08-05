// Business rules for stock changes: the adjustment invariant (never let
// stock go negative), and the fact that every adjustment must also produce
// a `StockMovement` audit record — the two collections are always written
// together from here, never independently.

use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        LowStockItem, LowStockResponse, StockAdjustmentRequest, StockAdjustmentResponse,
        StockMovementType, StockMovementsResponse,
    },
    modules::inventory::{model::StockMovementDocument, repository},
};

/// Applies a signed `delta` to a product's stock and records the change as
/// a `StockMovement`. Rejects with `INSUFFICIENT_STOCK` rather than
/// clamping to zero, since a caller requesting a delta larger than
/// available stock is almost always a bug (e.g. double-submitted sale)
/// that should surface as an error, not silently produce a wrong quantity.
pub(crate) async fn adjust_stock(
    db: &Database,
    id: ObjectId,
    body: StockAdjustmentRequest,
) -> AppResult<StockAdjustmentResponse> {
    let existing = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND))?;

    let previous_stock_quantity = existing.stock_quantity;
    let new_quantity = previous_stock_quantity + body.delta;

    if new_quantity < 0 {
        return Err(AppError::validation_with_code(
            format!(
                "Requested delta {} would result in negative stock (current stock: {previous_stock_quantity})",
                body.delta
            ),
            codes::INSUFFICIENT_STOCK,
        ));
    }

    let now = BsonDateTime::now();
    let updated = repository::product::adjust_product_stock(db, id, new_quantity, now)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND))?;

    repository::stock::insert_stock_movement(
        db,
        StockMovementDocument {
            id: None,
            key: generate_id(prefixes::STOCK_MOVEMENT),
            product_id: id,
            quantity_delta: body.delta,
            movement_type: StockMovementType::ManualAdjustment,
            reference_id: None,
            note: body.reason,
            created_at: now,
        },
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
}

/// Products at or below their configured reorder threshold.
pub(crate) async fn low_stock(db: &Database) -> AppResult<LowStockResponse> {
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
pub(crate) async fn product_movements(
    db: &Database,
    id: ObjectId,
) -> AppResult<StockMovementsResponse> {
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
