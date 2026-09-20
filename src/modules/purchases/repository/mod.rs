// Mongo access for the `purchases` collection only. Functions here never
// interpret a missing document as an error — they return `Vec`/counts
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can
// call in, but the `mod repository;` declaration in `purchases/mod.rs` is
// private, so none of this is reachable from outside this module tree.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    modules::purchases::model::PurchaseDocument,
};

fn purchases(db: &TenantDatabase) -> ScopedCollection<PurchaseDocument> {
    db.collection("purchases")
}

fn row_to_purchase_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<PurchaseDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let date_str: String = row.get("date");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(PurchaseDocument {
        id: object_id,
        key: row.get("key"),
        supplier_key: row.get("supplier_key"),
        product_key: row.get("product_key"),
        quantity: row.get("quantity"),
        unit_cost_cents: row.get("unit_cost_cents"),
        date: to_bson_datetime(&date_str),
        reference_no: row.get("reference_no"),
        notes: row.get("notes"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts `document` and backfills its `id` from the driver-generated
/// `_id`, since the caller builds the document with `id: None`.
pub(crate) async fn insert_purchase(
    db: &Db,
    mut document: PurchaseDocument,
) -> AppResult<PurchaseDocument> {
    match db {
        Db::Mongo(db) => {
            let result = purchases(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let date_iso = bson_to_iso(&document.date);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let deleted_at_iso = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO purchases (
                    key, id, supplier_key, product_key, quantity, unit_cost_cents,
                    date, reference_no, notes, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.supplier_key)
            .bind(&document.product_key)
            .bind(document.quantity)
            .bind(document.unit_cost_cents)
            .bind(&date_iso)
            .bind(&document.reference_no)
            .bind(&document.notes)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Sorted `date` descending — "newest first" per the frontend contract.
#[allow(dead_code)]
pub(crate) async fn list_purchases_by_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<PurchaseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = purchases(db)
                .find(doc! { "supplier_key": supplier_key })
                .sort(doc! { "date": -1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM purchases WHERE supplier_key = ? AND deleted_at IS NULL ORDER BY date DESC",
            )
            .bind(supplier_key)
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_purchase_doc).collect()
        }
    }
}

#[allow(dead_code)]
pub(crate) async fn list_purchases_by_product(
    db: &Db,
    product_key: &str,
) -> AppResult<Vec<PurchaseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = purchases(db)
                .find(doc! { "product_key": product_key })
                .sort(doc! { "date": -1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM purchases WHERE product_key = ? AND deleted_at IS NULL ORDER BY date DESC",
            )
            .bind(product_key)
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_purchase_doc).collect()
        }
    }
}

/// Backs the `SUPPLIER_HAS_PURCHASES` delete guard in
/// `suppliers::service::delete_supplier`.
pub(crate) async fn count_purchases_for_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(purchases(db)
            .count_documents(
                doc! { "supplier_key": supplier_key, "deleted_at": { "$exists": false } },
            )
            .await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM purchases WHERE supplier_key = ? AND deleted_at IS NULL",
            )
            .bind(supplier_key)
            .fetch_one(pool)
            .await?;

            Ok(count as u64)
        }
    }
}

pub(crate) async fn list_purchases_paginated(
    db: &Db,
    filter: mongodb::bson::Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<PurchaseDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let collection = purchases(db);
            let total = collection.count_documents(filter.clone()).await?;

            let mut cursor = collection
                .find(filter)
                .sort(doc! { "date": -1 })
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
            let supplier_key = filter.get_str("supplier_key").ok();
            let product_key = filter.get_str("product_key").ok();

            let mut count_query =
                String::from("SELECT COUNT(*) FROM purchases WHERE deleted_at IS NULL");
            let mut select_query = String::from("SELECT * FROM purchases WHERE deleted_at IS NULL");

            if supplier_key.is_some() {
                count_query.push_str(" AND supplier_key = ?");
                select_query.push_str(" AND supplier_key = ?");
            }
            if product_key.is_some() {
                count_query.push_str(" AND product_key = ?");
                select_query.push_str(" AND product_key = ?");
            }
            select_query.push_str(" ORDER BY date DESC LIMIT ? OFFSET ?");

            let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
            let mut select_q = sqlx::query(&select_query);

            if let Some(sk) = supplier_key {
                count_q = count_q.bind(sk);
                select_q = select_q.bind(sk);
            }
            if let Some(pk) = product_key {
                count_q = count_q.bind(pk);
                select_q = select_q.bind(pk);
            }

            select_q = select_q.bind(limit).bind(skip as i64);

            let total: i64 = count_q.fetch_one(pool).await?;
            let rows = select_q.fetch_all(pool).await?;
            let items = rows
                .iter()
                .map(row_to_purchase_doc)
                .collect::<AppResult<Vec<_>>>()?;

            Ok((items, total as u64))
        }
    }
}
