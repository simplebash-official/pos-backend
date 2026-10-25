// Dual-engine (Mongo / SQLite) access for the `receipt_settlements` collection: which payment
// paid how much of which receipt. Append-only: inserted and read, never edited or deleted.

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
    domain::purchase_receipts::PaymentKind,
    modules::purchases::model::ReceiptSettlementDocument,
};

fn settlements(db: &TenantDatabase) -> ScopedCollection<ReceiptSettlementDocument> {
    db.collection("receipt_settlements")
}

fn settlement_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<ReceiptSettlementDocument> {
    let kind: String = row.get("payment_kind");
    Ok(ReceiptSettlementDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        receipt_key: row.get("receipt_key"),
        supplier_key: row.get("supplier_key"),
        amount_cents: row.get("amount_cents"),
        payment_kind: PaymentKind::parse(&kind).ok_or_else(|| {
            AppError::internal(format!("Unknown payment kind '{kind}' in database"))
        })?,
        payment_key: row.get("payment_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts a settlement and backfills its `id`.
pub(crate) async fn insert_settlement(
    unit: impl Into<Unit<'_>>,
    mut document: ReceiptSettlementDocument,
) -> AppResult<ReceiptSettlementDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = settlements(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO receipt_settlements (
                    key, id, receipt_key, supplier_key, amount_cents, payment_kind, payment_key,
                    version, created_at, updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.receipt_key)
            .bind(&document.supplier_key)
            .bind(document.amount_cents)
            .bind(document.payment_kind.as_str())
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

/// Every settlement of the given receipts, oldest first.
pub(crate) async fn list_settlements_for_receipts(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<Vec<ReceiptSettlementDocument>> {
    if receipt_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = settlements(db)
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
                "SELECT * FROM receipt_settlements WHERE receipt_key IN ({placeholders}) ORDER BY created_at ASC, key ASC"
            );
            let mut query = sqlx::query(&sql);
            for key in receipt_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(settlement_from_row).collect()
        }
    }
}
