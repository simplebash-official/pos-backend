// Dual-engine (Mongo / SQLite) access for the `supplier_credits` collection. Append-only:
// credits are inserted and read, never edited or deleted; a void is another row.

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
    core::error::{AppError, AppResult},
    domain::supplier_credits::CreditReason,
    modules::purchases::model::SupplierCreditDocument,
};

fn credits(db: &TenantDatabase) -> ScopedCollection<SupplierCreditDocument> {
    db.collection("supplier_credits")
}

fn credit_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<SupplierCreditDocument> {
    let reason: String = row.get("reason");
    Ok(SupplierCreditDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        supplier_key: row.get("supplier_key"),
        receipt_key: row.get("receipt_key"),
        amount_cents: row.get("amount_cents"),
        reason: CreditReason::parse(&reason).ok_or_else(|| {
            AppError::internal(format!("Unknown credit reason '{reason}' in database"))
        })?,
        note: row.get("note"),
        reverses_key: row.get("reverses_key"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts a credit (or a voiding entry) and backfills its `id`.
pub(crate) async fn insert_credit(
    unit: impl Into<Unit<'_>>,
    mut document: SupplierCreditDocument,
) -> AppResult<SupplierCreditDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = credits(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO supplier_credits (
                    key, id, supplier_key, receipt_key, amount_cents, reason, note, reverses_key,
                    recorded_by_user_id, recorded_by_name_snapshot, version, created_at,
                    updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.supplier_key)
            .bind(&document.receipt_key)
            .bind(document.amount_cents)
            .bind(document.reason.as_str())
            .bind(&document.note)
            .bind(&document.reverses_key)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
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

/// One credit (or voiding entry) by key.
pub(crate) async fn find_credit_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SupplierCreditDocument>> {
    match db {
        Db::Mongo(db) => Ok(credits(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM supplier_credits WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(credit_from_row).transpose()
        }
    }
}

/// Newest first, optionally one supplier's and/or one receipt's, with the total for paging.
pub(crate) async fn list_credits_paginated(
    db: &Db,
    supplier_key: Option<&str>,
    receipt_key: Option<&str>,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<SupplierCreditDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! {};
            if let Some(key) = supplier_key {
                filter.insert("supplier_key", key);
            }
            if let Some(key) = receipt_key {
                filter.insert("receipt_key", key);
            }
            let collection = credits(db);
            let total = collection.count_documents(filter.clone()).await?;
            let mut cursor = collection
                .find(filter)
                .sort(doc! { "created_at": -1, "key": -1 })
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
            let mut conditions = Vec::new();
            if supplier_key.is_some() {
                conditions.push("supplier_key = ?");
            }
            if receipt_key.is_some() {
                conditions.push("receipt_key = ?");
            }
            let filter = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };
            let count_sql = format!("SELECT COUNT(*) FROM supplier_credits{filter}");
            let select_sql = format!(
                "SELECT * FROM supplier_credits{filter} ORDER BY created_at DESC, key DESC LIMIT ? OFFSET ?"
            );
            let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
            let mut select = sqlx::query(&select_sql);
            for value in [supplier_key, receipt_key].into_iter().flatten() {
                count = count.bind(value);
                select = select.bind(value);
            }
            let total = count.fetch_one(pool).await?;
            let rows = select.bind(limit).bind(skip as i64).fetch_all(pool).await?;
            Ok((
                rows.iter().map(credit_from_row).collect::<AppResult<_>>()?,
                total as u64,
            ))
        }
    }
}

/// Every credit and voiding entry against the given receipts.
pub(crate) async fn list_credits_for_receipts(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<Vec<SupplierCreditDocument>> {
    if receipt_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = credits(db)
                .find(doc! { "receipt_key": { "$in": receipt_keys } })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; receipt_keys.len()].join(", ");
            let sql =
                format!("SELECT * FROM supplier_credits WHERE receipt_key IN ({placeholders})");
            let mut query = sqlx::query(&sql);
            for key in receipt_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(credit_from_row).collect()
        }
    }
}

/// Every credit and voiding entry of one supplier, against a receipt or not.
pub(crate) async fn list_credits_for_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<SupplierCreditDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = credits(db)
                .find(doc! { "supplier_key": supplier_key })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM supplier_credits WHERE supplier_key = ?")
                .bind(supplier_key)
                .fetch_all(pool)
                .await?;
            rows.iter().map(credit_from_row).collect()
        }
    }
}
