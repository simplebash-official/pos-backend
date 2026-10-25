// Dual-engine (Mongo / SQLite) access for the `receipt_charges` collection: the extra charges
// (shipping, duty, handling, insurance, discounts) on a receipt. Append-only: inserted and read,
// never edited or deleted.

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
    domain::purchase_receipts::{Allocation, BilledBy, ChargeKind, PaymentKind},
    modules::purchases::model::ReceiptChargeDocument,
};

fn charges(db: &TenantDatabase) -> ScopedCollection<ReceiptChargeDocument> {
    db.collection("receipt_charges")
}

fn charge_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<ReceiptChargeDocument> {
    let kind: String = row.get("kind");
    let billed_by: String = row.get("billed_by");
    let allocation: Option<String> = row.get("allocation");
    let payment_kind: Option<String> = row.get("payment_kind");
    Ok(ReceiptChargeDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        receipt_key: row.get("receipt_key"),
        supplier_key: row.get("supplier_key"),
        date: to_bson_datetime(&row.get::<String, _>("date")),
        kind: ChargeKind::parse(&kind).ok_or_else(|| {
            AppError::internal(format!("Unknown charge kind '{kind}' in database"))
        })?,
        amount_cents: row.get("amount_cents"),
        billed_by: BilledBy::parse(&billed_by).ok_or_else(|| {
            AppError::internal(format!("Unknown charge biller '{billed_by}' in database"))
        })?,
        allocation: allocation
            .map(|a| {
                Allocation::parse(&a).ok_or_else(|| {
                    AppError::internal(format!("Unknown allocation '{a}' in database"))
                })
            })
            .transpose()?,
        payee_key: row.get("payee_key"),
        payment_kind: payment_kind
            .map(|k| {
                PaymentKind::parse(&k).ok_or_else(|| {
                    AppError::internal(format!("Unknown payment kind '{k}' in database"))
                })
            })
            .transpose()?,
        payment_key: row.get("payment_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts a charge and backfills its `id`.
pub(crate) async fn insert_charge(
    unit: impl Into<Unit<'_>>,
    mut document: ReceiptChargeDocument,
) -> AppResult<ReceiptChargeDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = charges(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO receipt_charges (
                    key, id, receipt_key, supplier_key, date, kind, amount_cents, billed_by,
                    allocation, payee_key, payment_kind, payment_key,
                    version, created_at, updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.receipt_key)
            .bind(&document.supplier_key)
            .bind(bson_to_iso(&document.date))
            .bind(document.kind.as_str())
            .bind(document.amount_cents)
            .bind(document.billed_by.as_str())
            .bind(document.allocation.map(|a| a.as_str()))
            .bind(&document.payee_key)
            .bind(document.payment_kind.map(|k| k.as_str()))
            .bind(&document.payment_key)
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

/// Every charge of the given receipts, oldest first.
pub(crate) async fn list_charges_for_receipts(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<Vec<ReceiptChargeDocument>> {
    if receipt_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = charges(db)
                .find(doc! { "receipt_key": { "$in": receipt_keys } })
                .sort(doc! { "created_at": 1, "key": 1 })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; receipt_keys.len()].join(", ");
            let sql = format!(
                "SELECT * FROM receipt_charges WHERE receipt_key IN ({placeholders}) ORDER BY created_at ASC, key ASC"
            );
            let mut query = sqlx::query(&sql);
            for key in receipt_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(charge_from_row).collect()
        }
    }
}

/// Every charge the supplier put on its bills, for the supplier balance.
pub(crate) async fn list_supplier_billed_charges(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<ReceiptChargeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = charges(db)
                .find(doc! { "supplier_key": supplier_key, "billed_by": BilledBy::Supplier.as_str() })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM receipt_charges WHERE supplier_key = ? AND billed_by = 'supplier'",
            )
            .bind(supplier_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(charge_from_row).collect()
        }
    }
}
