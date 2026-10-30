use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::{AppError, AppResult};
use crate::domain::purchase_receipts::{Allocation, BilledBy, ChargeKind, PaymentKind};
use crate::modules::purchases::model::ReceiptChargeDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn charge_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<ReceiptChargeDocument> {
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
        description: row.get("description"),
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
