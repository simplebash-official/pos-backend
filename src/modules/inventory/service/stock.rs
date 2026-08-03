use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, oid::ObjectId},
};

use crate::{
    core::{
        constants::prefixes,
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        LowStockItem, LowStockResponse, StockAdjustmentRequest, StockAdjustmentResponse,
        StockMovementType, StockMovementsResponse,
    },
    modules::inventory::{model::StockMovementDocument, repository},
};

pub(crate) async fn adjust_stock(
    db: &Database,
    id: ObjectId,
    body: StockAdjustmentRequest,
) -> AppResult<StockAdjustmentResponse> {
    let existing = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    let previous_stock_quantity = existing.stock_quantity;
    let new_quantity = previous_stock_quantity + body.delta;

    if new_quantity < 0 {
        return Err(AppError::validation_with_code(
            format!(
                "Requested delta {} would result in negative stock (current stock: {previous_stock_quantity})",
                body.delta
            ),
            "INSUFFICIENT_STOCK",
        ));
    }

    let now = BsonDateTime::now();
    let updated = repository::product::adjust_product_stock(db, id, new_quantity, now)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

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
            "PRODUCT_NOT_FOUND",
        ));
    }

    let documents = repository::stock::find_stock_movements_for_product(db, id).await?;
    let movements = documents
        .into_iter()
        .map(StockMovementDocument::into_stock_movement)
        .collect();

    Ok(StockMovementsResponse { movements })
}
