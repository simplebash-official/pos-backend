use super::settlements::settlements;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::{AppError, AppResult};
use crate::domain::purchase_receipts::PaymentKind;
use crate::modules::purchases::model::ReceiptSettlementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

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
