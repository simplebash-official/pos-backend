use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::domain::billing::{InvoiceItem, InvoiceStatus, SplitPayment};
use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(crate) fn invoice_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<InvoiceDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let items_str: String = row.get("items");
    let items: Vec<InvoiceItem> = serde_json::from_str(&items_str).unwrap_or_default();
    let split_payments_str: Option<String> = row.get("split_payments");
    let split_payments: Option<Vec<SplitPayment>> =
        split_payments_str.and_then(|s| serde_json::from_str(&s).ok());
    let is_credit_int: i64 = row.get("is_credit");
    let status_str: String = row.get("status");
    let status: InvoiceStatus = serde_json::from_value(serde_json::Value::String(status_str))
        .unwrap_or(InvoiceStatus::Paid);
    let shop_profile_str: String = row.get("shop_profile_snapshot");
    let shop_profile_snapshot: serde_json::Value =
        serde_json::from_str(&shop_profile_str).unwrap_or(serde_json::Value::Null);

    let voided_at_str: Option<String> = row.get("voided_at");
    let closed_at_str: Option<String> = row.get("closed_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(InvoiceDocument {
        id,
        key: row.get("key"),
        invoice_number: row.get("invoice_number"),
        customer_key: row.get("customer_key"),
        customer_name_snapshot: row.get("customer_name_snapshot"),
        customer_phone_snapshot: row.get("customer_phone_snapshot"),
        customer_address_snapshot: row.get("customer_address_snapshot"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        items,
        subtotal_cents: row.get("subtotal_cents"),
        discount_type: row.get("discount_type"),
        discount_value: row.get("discount_value"),
        discount_cents: row.get("discount_cents"),
        total_cents: row.get("total_cents"),
        payment_method: row.get("payment_method"),
        split_payments,
        is_credit: is_credit_int != 0,
        amount_received_cents: row.get("amount_received_cents"),
        change_due_cents: row.get("change_due_cents"),
        due_date: row.get("due_date"),
        card_last4: row.get("card_last4"),
        card_ref: row.get("card_ref"),
        online_ref: row.get("online_ref"),
        online_note: row.get("online_note"),
        status,
        notes: row.get("notes"),
        shop_profile_snapshot,
        warranty_terms_snapshot: row.get("warranty_terms_snapshot"),
        document_selection: row.get("document_selection"),
        voided_at: voided_at_str.map(|s| to_bson_datetime(&s)),
        voided_by: row.get("voided_by"),
        voided_reason: row.get("voided_reason"),
        closed_at: closed_at_str.map(|s| to_bson_datetime(&s)),
        closed_by: row.get("closed_by"),
        refunded_cents: row.get("refunded_cents"),
        credit_note_count: row.get("credit_note_count"),
        session_key: row.get("session_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        updated_by_device: row.get("updated_by_device"),
    })
}
