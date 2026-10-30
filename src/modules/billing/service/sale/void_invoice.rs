use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::billing::{Invoice, InvoiceStatus, VoidInvoiceRequest};
use crate::domain::inventory::StockMovementType;

use crate::modules::billing::repository;
use crate::modules::{customers, inventory};
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Void (D8): reverses retail stock and customer financials, sets
/// `status: Voided` with a mandatory reason and audit trail. This is the
/// single "invalidated after the fact" action — see
/// `InvoiceStatus::Voided`'s doc comment for why there is no separate
/// zero-impact "Cancelled" status in this codebase. Does **not** touch
/// repair/print-job ticket status (ambiguous what "un-delivering" a ticket
/// should mean — flagged as a warning for manual follow-up instead) and
/// refuses to run at all if any payment beyond the original sale-time one
/// has been recorded (reversing partial credit repayments is out of scope
/// for this version).
pub async fn void_invoice(
    db: &Db,
    id_or_key: &str,
    body: VoidInvoiceRequest,
    voided_by: String,
) -> AppResult<(Invoice, Vec<String>)> {
    crate::core::logging::domain::tracked("billing.invoice_voided", async move {
        if body.reason.trim().is_empty() {
            return Err(AppError::validation(
                "A reason is required to void an invoice",
            ));
        }

        let existing = repository::find_invoice_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        if existing.status == InvoiceStatus::Voided {
            return Err(AppError::conflict(
                codes::INVOICE_ALREADY_VOIDED,
                "This invoice has already been voided",
            ));
        }

        // How many payment rows `complete_sale` itself would have inserted —
        // for a credit sale, one if a deposit was taken at checkout else zero;
        // one per split leg for a split payment; otherwise exactly one. Any
        // payment beyond that count means a repayment (or credit-note refund)
        // has since been recorded, which this basic void flow cannot safely
        // reverse.
        let expected_payment_count: usize = if existing.is_credit {
            usize::from(existing.amount_received_cents.unwrap_or(0) > 0)
        } else if existing.payment_method == "split" {
            existing
                .split_payments
                .as_ref()
                .map_or(1, |legs| legs.len())
        } else {
            1
        };
        let payment_count = repository::count_payments_for_invoice(db, &existing.key).await?;
        if payment_count as usize > expected_payment_count {
            return Err(AppError::conflict(
                codes::INVOICE_HAS_PAYMENTS_CANNOT_VOID,
                "This invoice has payments recorded beyond the original sale and cannot be voided automatically",
            ));
        }

        let object_id = existing
            .id
            .expect("persisted invoice document must have an _id");
        let mut warnings: Vec<String> = Vec::new();

        for item in &existing.items {
            if item.source_type != "retail" {
                if matches!(item.source_type.as_str(), "repair" | "print") {
                    warnings.push(format!(
                        "Invoice voided — ticket for '{}' was not reverted and may need manual review",
                        item.name
                    ));
                }
                continue;
            }
            let Some(product_key) = item.product_key.as_deref() else {
                continue;
            };
            if let Err(err) = inventory::service::stock_batch::move_allocations(
                db,
                product_key,
                &item.batch_allocations,
                1,
                StockMovementType::InvoiceVoidReversal,
                &existing.key,
                &format!("Void of invoice {}", existing.invoice_number),
            )
            .await
            {
                warnings.push(format!(
                    "Stock for '{}' could not be restored: {err}",
                    item.name
                ));
            }
        }

        if let Some(customer_key) = &existing.customer_key {
            // Only the amount that actually landed on the customer's account
            // (total minus any checkout deposit) was ever added to their owed
            // balance, so only that much is reversed. The deposit payment row
            // itself is left in place (same as a voided cash sale keeps its
            // sale-time payment row).
            let balance_delta = if existing.is_credit {
                -(existing.total_cents - existing.amount_received_cents.unwrap_or(0))
            } else {
                0
            };
            if let Err(err) = customers::service::apply_financial_delta(
                db,
                customer_key,
                -existing.total_cents,
                balance_delta,
            )
            .await
            {
                warnings.push(format!("Customer financials could not be reversed: {err}"));
            }
        }

        let set_doc = doc! {
            "status": InvoiceStatus::Voided.as_str(),
            "voided_at": BsonDateTime::now(),
            "voided_by": &voided_by,
            "voided_reason": &body.reason,
            "updated_at": BsonDateTime::now(),
        };

        let updated = repository::update_invoice_status(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        Ok((updated.into_invoice(), warnings))
    })
    .await
}
