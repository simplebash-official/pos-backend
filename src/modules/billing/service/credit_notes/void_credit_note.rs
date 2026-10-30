use super::line_hint::LineHint;
use super::line_match::line_match;
use crate::clients::db::Db;

use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::billing::{
    CreditNote, CreditNoteStatus, InvoiceItem, ItemCondition, VoidCreditNoteRequest,
};
use crate::domain::inventory::StockMovementType;

use crate::modules::billing::model::PaymentDocument;
use crate::modules::billing::repository;
use crate::modules::inventory;

use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Reverses a credit note's stock/payment/customer-balance effects and marks
/// it `Voided` with a mandatory reason — mirrors `sale::void_invoice`'s
/// posture. Reverses `ReturnRestock` movements (deducts the quantity back
/// out) but does not attempt to reverse `ReturnWriteOff`/`ReturnSupplierRma`
/// (audit-only, no quantity change to undo) or re-credit exchange stock
/// (out of scope for this pass — voiding an exchange's replacement item sale
/// is not a returns flow).
pub async fn void_credit_note(
    db: &Db,
    id_or_key: &str,
    body: VoidCreditNoteRequest,
    voided_by: String,
) -> AppResult<CreditNote> {
    crate::core::logging::domain::tracked("billing.credit_note_voided", async move {
        if body.reason.trim().is_empty() {
            return Err(AppError::validation(
                "A reason is required to void a credit note",
            ));
        }

        let existing = repository::credit_notes::find_credit_note_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Credit note not found", codes::CREDIT_NOTE_NOT_FOUND)
            })?;

        if existing.status == CreditNoteStatus::Voided {
            return Err(AppError::conflict(
                codes::CREDIT_NOTE_ALREADY_VOIDED,
                "This credit note has already been voided",
            ));
        }

        for item in &existing.returned_items {
            if item.condition != ItemCondition::Resalable
                && item.condition != ItemCondition::OpenBoxDiscount
            {
                continue;
            }
            let Some(product_key) = &item.product_key else {
                continue;
            };
            if let Err(err) = inventory::service::stock_batch::move_allocations(
                db,
                product_key,
                &item.batch_allocations,
                -1,
                StockMovementType::ReturnRestock,
                &existing.key,
                &format!("Void of credit note {}", existing.credit_note_number),
            )
            .await
            {
                tracing::warn!(product_key, error = %err, "could not reverse credit note restock");
            }
        }

        for payment_key in &existing.refund_payment_keys {
            if let Ok(Some(original)) = repository::find_payment_by_key(db, payment_key).await {
                let now = BsonDateTime::now();
                let reversal = PaymentDocument {
                    id: None,
                    key: generate_id(prefixes::PAYMENT),
                    invoice_key: original.invoice_key.clone(),
                    amount_cents: -original.amount_cents,
                    payment_method: original.payment_method.clone(),
                    notes: Some(format!(
                        "Reversal of credit note {} (voided)",
                        existing.credit_note_number
                    )),
                    session_key: original.session_key.clone(),
                    recorded_by_user_id: voided_by.clone(),
                    recorded_by_name_snapshot: voided_by.clone(),
                    recorded_at: now,
                    version: 1,
                    created_at: now,
                    updated_at: now,
                };
                if let Err(err) = repository::insert_payment(db, reversal).await {
                    tracing::warn!(payment_key, error = %err, "could not reverse credit note refund payment");
                }
            }
        }

        if let Some(invoice_key) = &existing.invoice_key
            && let Ok(Some(invoice)) = repository::find_invoice_by_id_or_key(db, invoice_key).await
            && let Some(invoice_id) = invoice.id
        {
            let invoice_version = invoice.version;
            // Each returned line gives its quantity back to the one invoice
            // line it came from (a product sold at two prices is on two lines).
            let mut restored_items: Vec<InvoiceItem> = invoice.items;
            let sole = restored_items.len() == 1 && existing.returned_items.len() == 1;
            for returned in &existing.returned_items {
                let batch_keys: Vec<&str> = returned
                    .batch_allocations
                    .iter()
                    .map(|a| a.batch_key.as_str())
                    .collect();
                if let Some(idx) = line_match(
                    &restored_items,
                    returned.product_key.as_deref(),
                    returned.source_ticket_key.as_deref(),
                    Some(&returned.name),
                    LineHint {
                        unit_price_cents: None,
                        batch_keys: &batch_keys,
                    },
                    sole,
                ) {
                    let line = &mut restored_items[idx];
                    line.returned_quantity = (line.returned_quantity - returned.quantity).max(0);
                }
            }
            let refunded_delta =
                -(existing.refund_cash_cents.max(0) + existing.balance_reduction_cents.max(0));
            // A version mismatch here (`Ok(None)`) is treated as a best-effort
            // miss, not retried — voiding is a much lower-stakes reversal than
            // creation, and a concurrent write losing this decrement is already
            // logged for investigation rather than silently accepted.
            if let Err(err) = repository::invoice::update_invoice_credit_note_progress(
                db,
                invoice_id,
                restored_items,
                refunded_delta,
                -1,
                invoice_version,
            )
            .await
            {
                tracing::warn!(invoice_key, error = %err, "could not reverse invoice credit-note progress on void");
            }
        }

        let object_id = existing
            .id
            .expect("persisted credit note document must have an _id");
        let set_doc = doc! {
            "status": "voided",
            "voided_at": BsonDateTime::now(),
            "voided_by": &voided_by,
            "voided_reason": &body.reason,
            "updated_at": BsonDateTime::now(),
        };
        let updated = repository::credit_notes::update_credit_note_status(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Credit note not found", codes::CREDIT_NOTE_NOT_FOUND)
            })?;

        Ok(updated.into_credit_note())
    })
    .await
}
