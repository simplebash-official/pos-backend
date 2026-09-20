// Mongo access for the `stock_movements` collection only — the append-only
// audit trail behind every stock change. Movements are never updated or
// deleted here, only inserted and read.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::{bson::doc, bson::oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    domain::inventory::StockMovementType,
    modules::inventory::model::StockMovementDocument,
};

fn stock_movements(db: &TenantDatabase) -> ScopedCollection<StockMovementDocument> {
    db.collection("stock_movements")
}

fn row_to_stock_movement_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<StockMovementDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let product_id_str: String = row.get("product_id");
    let product_id = ObjectId::parse_str(&product_id_str).unwrap_or_default();
    let movement_type_str: String = row.get("movement_type");
    let movement_type = movement_type_str
        .parse()
        .unwrap_or(StockMovementType::ManualAdjustment);
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(StockMovementDocument {
        id: object_id,
        key: row.get("key"),
        product_id,
        quantity_delta: row.get("quantity_delta"),
        movement_type,
        reference_id: row.get("reference_id"),
        note: row.get("note"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

pub(crate) async fn insert_stock_movement(
    db: &Db,
    movement: StockMovementDocument,
) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            stock_movements(db).insert_one(&movement).await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let id = movement
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&movement.created_at);
            let updated_at_iso = bson_to_iso(&movement.updated_at);
            let deleted_at_iso = movement.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO stock_movements (
                    key, id, product_id, quantity_delta, movement_type, reference_id,
                    note, version, created_at, updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&movement.key)
            .bind(&id)
            .bind(movement.product_id.to_hex())
            .bind(movement.quantity_delta)
            .bind(movement.movement_type.as_str())
            .bind(&movement.reference_id)
            .bind(&movement.note)
            .bind(movement.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&movement.updated_by_device)
            .execute(pool)
            .await?;

            Ok(())
        }
    }
}

/// Sorted oldest-first so `GET /products/{id}/movements` reads as a
/// chronological history rather than most-recent-first.
pub(crate) async fn find_stock_movements_for_product(
    db: &Db,
    product_id: ObjectId,
) -> AppResult<Vec<StockMovementDocument>> {
    match db {
        Db::Mongo(db) => {
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
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM stock_movements WHERE product_id = ? AND deleted_at IS NULL ORDER BY created_at ASC",
            )
            .bind(product_id.to_hex())
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_stock_movement_doc).collect()
        }
    }
}

pub(crate) async fn list_stock_movements_paginated(
    db: &Db,
    filter: mongodb::bson::Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<StockMovementDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
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
        Db::Sqlite(pool) => {
            let product_id = filter.get_object_id("product_id").ok();
            let movement_type = filter.get_str("movement_type").ok();

            let mut count_query =
                String::from("SELECT COUNT(*) FROM stock_movements WHERE deleted_at IS NULL");
            let mut select_query =
                String::from("SELECT * FROM stock_movements WHERE deleted_at IS NULL");

            if product_id.is_some() {
                count_query.push_str(" AND product_id = ?");
                select_query.push_str(" AND product_id = ?");
            }
            if movement_type.is_some() {
                count_query.push_str(" AND movement_type = ?");
                select_query.push_str(" AND movement_type = ?");
            }

            select_query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

            let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
            let mut select_q = sqlx::query(&select_query);

            if let Some(pid) = product_id {
                let pid_hex = pid.to_hex();
                count_q = count_q.bind(pid_hex.clone());
                select_q = select_q.bind(pid_hex);
            }
            if let Some(mt) = movement_type {
                count_q = count_q.bind(mt);
                select_q = select_q.bind(mt);
            }

            select_q = select_q.bind(limit).bind(skip as i64);

            let total: i64 = count_q.fetch_one(pool).await?;
            let rows = select_q.fetch_all(pool).await?;
            let items = rows
                .iter()
                .map(row_to_stock_movement_doc)
                .collect::<AppResult<Vec<_>>>()?;

            Ok((items, total as u64))
        }
    }
}
