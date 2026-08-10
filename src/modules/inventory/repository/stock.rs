// Mongo access for the `stock_movements` collection only — the append-only
// audit trail behind every stock change. Movements are never updated or
// deleted here, only inserted and read.

use futures_util::TryStreamExt;
use mongodb::{Collection, Database, bson::doc, bson::oid::ObjectId};

use crate::{core::error::AppResult, modules::inventory::model::StockMovementDocument};

fn stock_movements(db: &Database) -> Collection<StockMovementDocument> {
    db.collection("stock_movements")
}

pub(crate) async fn insert_stock_movement(
    db: &Database,
    movement: StockMovementDocument,
) -> AppResult<()> {
    stock_movements(db).insert_one(&movement).await?;
    Ok(())
}

/// Sorted oldest-first so `GET /products/{id}/movements` reads as a
/// chronological history rather than most-recent-first.
pub(crate) async fn find_stock_movements_for_product(
    db: &Database,
    product_id: ObjectId,
) -> AppResult<Vec<StockMovementDocument>> {
    let mut cursor = stock_movements(db)
        .find(doc! { "product_id": product_id, "deleted_at": { "$exists": false } })
        .sort(doc! { "created_at": 1 })
        .await?;

    let mut movements = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        movements.push(document);
    }
    Ok(movements)
}

pub(crate) async fn list_stock_movements_paginated(
    db: &Database,
    filter: mongodb::bson::Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<StockMovementDocument>, u64)> {
    let collection = stock_movements(db);
    let total = collection.count_documents(filter.clone()).await?;

    let mut cursor = collection
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .skip(skip)
        .limit(limit)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}
