use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::domain::billing::{CreditNoteStatus, InvoiceItem, RefundBreakdownLeg};
use crate::modules::billing::model::{CreditNoteDocument, CreditNoteItemDocument};

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(crate) fn credit_note_from_sqlite_row(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<CreditNoteDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let no_receipt_int: i64 = row.get("no_receipt");
    let returned_items_str: String = row.get("returned_items");
    let returned_items: Vec<CreditNoteItemDocument> =
        serde_json::from_str(&returned_items_str).unwrap_or_default();
    let exchange_items_str: String = row.get("exchange_items");
    let exchange_items: Vec<InvoiceItem> =
        serde_json::from_str(&exchange_items_str).unwrap_or_default();
    let refund_breakdown_str: String = row.get("refund_breakdown");
    let refund_breakdown: Vec<RefundBreakdownLeg> =
        serde_json::from_str(&refund_breakdown_str).unwrap_or_default();
    let refund_payment_keys_str: String = row.get("refund_payment_keys");
    let refund_payment_keys: Vec<String> =
        serde_json::from_str(&refund_payment_keys_str).unwrap_or_default();
    let status_str: String = row.get("status");
    let status: CreditNoteStatus = serde_json::from_value(serde_json::Value::String(status_str))
        .unwrap_or(CreditNoteStatus::Resolved);
    let is_manager_override_int: i64 = row.get("is_manager_override");

    let voided_at_str: Option<String> = row.get("voided_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(CreditNoteDocument {
        id,
        key: row.get("key"),
        credit_note_number: row.get("credit_note_number"),
        invoice_key: row.get("invoice_key"),
        invoice_number: row.get("invoice_number"),
        no_receipt: no_receipt_int != 0,
        customer_key: row.get("customer_key"),
        customer_name_snapshot: row.get("customer_name_snapshot"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        returned_items,
        exchange_items,
        exchange_reference: row.get("exchange_reference"),
        return_subtotal_cents: row.get("return_subtotal_cents"),
        exchange_subtotal_cents: row.get("exchange_subtotal_cents"),
        net_refund_cents: row.get("net_refund_cents"),
        refund_cash_cents: row.get("refund_cash_cents"),
        balance_reduction_cents: row.get("balance_reduction_cents"),
        refund_breakdown,
        refund_payment_keys,
        status,
        is_manager_override: is_manager_override_int != 0,
        override_approved_by: row.get("override_approved_by"),
        override_reason: row.get("override_reason"),
        notes: row.get("notes"),
        session_key: row.get("session_key"),
        voided_at: voided_at_str.map(|s| to_bson_datetime(&s)),
        voided_by: row.get("voided_by"),
        voided_reason: row.get("voided_reason"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}
