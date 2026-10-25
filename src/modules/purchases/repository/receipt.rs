// Dual-engine (Mongo / SQLite) access for the `purchase_receipts` collection. Receipts are
// append-only: inserted and read, never edited or deleted. Like the rest of this module, nothing
// here turns a missing document into an error; `service` does that.

use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
        uow::{Backend, Unit},
    },
    core::error::AppResult,
    modules::purchases::model::PurchaseReceiptDocument,
};

fn receipts(db: &TenantDatabase) -> ScopedCollection<PurchaseReceiptDocument> {
    db.collection("purchase_receipts")
}

fn receipt_from_row(row: &sqlx::sqlite::SqliteRow) -> PurchaseReceiptDocument {
    PurchaseReceiptDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        receipt_number: row.get("receipt_number"),
        supplier_key: row.get("supplier_key"),
        reference_no: row.get("reference_no"),
        date: to_bson_datetime(&row.get::<String, _>("date")),
        notes: row.get("notes"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    }
}

/// Inserts a receipt and backfills its `id`.
pub(crate) async fn insert_receipt(
    unit: impl Into<Unit<'_>>,
    mut document: PurchaseReceiptDocument,
) -> AppResult<PurchaseReceiptDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = receipts(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO purchase_receipts (
                    key, id, receipt_number, supplier_key, reference_no, date, notes,
                    version, created_at, updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.receipt_number)
            .bind(&document.supplier_key)
            .bind(&document.reference_no)
            .bind(bson_to_iso(&document.date))
            .bind(&document.notes)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.updated_by_device)
            .execute(&mut *conn)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Fetches one receipt by its key.
pub(crate) async fn find_receipt_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<PurchaseReceiptDocument>> {
    match db {
        Db::Mongo(db) => Ok(receipts(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM purchase_receipts WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(receipt_from_row))
        }
    }
}

/// The supplier's earlier receipt carrying the same invoice number, if any (the duplicate check).
pub(crate) async fn find_receipt_by_supplier_and_reference(
    db: &Db,
    supplier_key: &str,
    reference_no: &str,
) -> AppResult<Option<PurchaseReceiptDocument>> {
    match db {
        Db::Mongo(db) => Ok(receipts(db)
            .find_one(doc! { "supplier_key": supplier_key, "reference_no": reference_no })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM purchase_receipts WHERE supplier_key = ? AND reference_no = ? LIMIT 1",
            )
            .bind(supplier_key)
            .bind(reference_no)
            .fetch_optional(pool)
            .await?;
            Ok(row.as_ref().map(receipt_from_row))
        }
    }
}

/// Newest first, optionally one supplier's, with the total for paging.
pub(crate) async fn list_receipts_paginated(
    db: &Db,
    supplier_key: Option<&str>,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<PurchaseReceiptDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let filter = match supplier_key {
                Some(key) => doc! { "supplier_key": key },
                None => doc! {},
            };
            let collection = receipts(db);
            let total = collection.count_documents(filter.clone()).await?;
            let mut cursor = collection
                .find(filter)
                .sort(doc! { "date": -1, "created_at": -1 })
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
            let filter = if supplier_key.is_some() {
                " WHERE supplier_key = ?"
            } else {
                ""
            };
            let count_sql = format!("SELECT COUNT(*) FROM purchase_receipts{filter}");
            let select_sql = format!(
                "SELECT * FROM purchase_receipts{filter} ORDER BY date DESC, created_at DESC LIMIT ? OFFSET ?"
            );
            let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
            let mut select = sqlx::query(&select_sql);
            if let Some(key) = supplier_key {
                count = count.bind(key);
                select = select.bind(key);
            }
            let total = count.fetch_one(pool).await?;
            let rows = select.bind(limit).bind(skip as i64).fetch_all(pool).await?;
            Ok((rows.iter().map(receipt_from_row).collect(), total as u64))
        }
    }
}
