use super::credit_notes_collection::credit_notes_collection;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;
use crate::modules::billing::model::CreditNoteDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts a new credit note document and populates its auto-generated `_id`.
pub(crate) async fn insert_credit_note(
    db: &Db,
    mut document: CreditNoteDocument,
) -> AppResult<CreditNoteDocument> {
    match db {
        Db::Mongo(db) => {
            let result = credit_notes_collection(db).insert_one(&document).await?;
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
            let returned_items_json = serde_json::to_string(&document.returned_items)
                .unwrap_or_else(|_| "[]".to_string());
            let exchange_items_json = serde_json::to_string(&document.exchange_items)
                .unwrap_or_else(|_| "[]".to_string());
            let refund_breakdown_json = serde_json::to_string(&document.refund_breakdown)
                .unwrap_or_else(|_| "[]".to_string());
            let refund_payment_keys_json = serde_json::to_string(&document.refund_payment_keys)
                .unwrap_or_else(|_| "[]".to_string());
            let status_str = serde_json::to_value(document.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "approved".to_string());

            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let voided_at_iso = document.voided_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO credit_notes (
                    key, id, credit_note_number, invoice_key, invoice_number, no_receipt,
                    customer_key, customer_name_snapshot, cashier_id, cashier_name_snapshot,
                    returned_items, exchange_items, exchange_reference, return_subtotal_cents,
                    exchange_subtotal_cents, net_refund_cents, refund_cash_cents,
                    balance_reduction_cents, refund_breakdown, refund_payment_keys, status,
                    is_manager_override, override_approved_by, override_reason, notes,
                    session_key, voided_at, voided_by, voided_reason, version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.credit_note_number)
            .bind(&document.invoice_key)
            .bind(&document.invoice_number)
            .bind(if document.no_receipt { 1 } else { 0 })
            .bind(&document.customer_key)
            .bind(&document.customer_name_snapshot)
            .bind(&document.cashier_id)
            .bind(&document.cashier_name_snapshot)
            .bind(&returned_items_json)
            .bind(&exchange_items_json)
            .bind(&document.exchange_reference)
            .bind(document.return_subtotal_cents)
            .bind(document.exchange_subtotal_cents)
            .bind(document.net_refund_cents)
            .bind(document.refund_cash_cents)
            .bind(document.balance_reduction_cents)
            .bind(&refund_breakdown_json)
            .bind(&refund_payment_keys_json)
            .bind(&status_str)
            .bind(if document.is_manager_override { 1 } else { 0 })
            .bind(&document.override_approved_by)
            .bind(&document.override_reason)
            .bind(&document.notes)
            .bind(&document.session_key)
            .bind(&voided_at_iso)
            .bind(&document.voided_by)
            .bind(&document.voided_reason)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
