use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;

use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_invoice(
    db: &Db,
    mut document: InvoiceDocument,
) -> AppResult<InvoiceDocument> {
    match db {
        Db::Mongo(db) => {
            let result = invoices(db).insert_one(&document).await?;
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
            let items_json =
                serde_json::to_string(&document.items).unwrap_or_else(|_| "[]".to_string());
            let split_payments_json = document
                .split_payments
                .as_ref()
                .map(|sp| serde_json::to_string(sp).unwrap_or_else(|_| "[]".to_string()));
            let shop_profile_json = serde_json::to_string(&document.shop_profile_snapshot)
                .unwrap_or_else(|_| "{}".to_string());
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let voided_at_iso = document.voided_at.as_ref().map(bson_to_iso);
            let closed_at_iso = document.closed_at.as_ref().map(bson_to_iso);
            let status_str = serde_json::to_value(document.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "paid".to_string());

            sqlx::query(
                r#"
                INSERT INTO invoices (
                    key, id, invoice_number, customer_key, customer_name_snapshot,
                    customer_phone_snapshot, customer_address_snapshot, cashier_id,
                    cashier_name_snapshot, items, subtotal_cents, discount_type,
                    discount_value, discount_cents, total_cents, payment_method,
                    split_payments, is_credit, amount_received_cents, change_due_cents,
                    due_date, card_last4, card_ref, online_ref, online_note, status,
                    notes, shop_profile_snapshot, warranty_terms_snapshot, document_selection,
                    voided_at, voided_by, voided_reason, closed_at, closed_by,
                    refunded_cents, credit_note_count, session_key, version, created_at, updated_at, updated_by_device
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.invoice_number)
            .bind(&document.customer_key)
            .bind(&document.customer_name_snapshot)
            .bind(&document.customer_phone_snapshot)
            .bind(&document.customer_address_snapshot)
            .bind(&document.cashier_id)
            .bind(&document.cashier_name_snapshot)
            .bind(&items_json)
            .bind(document.subtotal_cents)
            .bind(&document.discount_type)
            .bind(document.discount_value)
            .bind(document.discount_cents)
            .bind(document.total_cents)
            .bind(&document.payment_method)
            .bind(&split_payments_json)
            .bind(if document.is_credit { 1 } else { 0 })
            .bind(document.amount_received_cents)
            .bind(document.change_due_cents)
            .bind(&document.due_date)
            .bind(&document.card_last4)
            .bind(&document.card_ref)
            .bind(&document.online_ref)
            .bind(&document.online_note)
            .bind(&status_str)
            .bind(&document.notes)
            .bind(&shop_profile_json)
            .bind(&document.warranty_terms_snapshot)
            .bind(&document.document_selection)
            .bind(&voided_at_iso)
            .bind(&document.voided_by)
            .bind(&document.voided_reason)
            .bind(&closed_at_iso)
            .bind(&document.closed_by)
            .bind(document.refunded_cents)
            .bind(document.credit_note_count)
            .bind(&document.session_key)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
