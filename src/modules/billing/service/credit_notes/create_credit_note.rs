use super::service_credit_notes::LineHint;
use super::service_credit_notes::line_match;
use crate::clients::db::Db;
use crate::core::calculations;
use crate::core::config::Config;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::billing::{
    CreateCreditNoteItemRequest, CreateCreditNoteRequest, CreateSaleItemRequest, CreditNote,
    CreditNoteItem, CreditNoteStatus, InvoiceItem, InvoiceStatus, ItemCondition, ItemDisposition,
    RefundBreakdownLeg,
};
use crate::domain::inventory::{BatchAllocation, Product, StockMovementType};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::users::Role;
use crate::modules::billing::model::{CreditNoteDocument, PaymentDocument};
use crate::modules::billing::repository;
use crate::modules::{customers, inventory, sequences, warranties};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use std::collections::HashMap;

/// Enforces the `disposition` iff `condition == Damaged` rule from
/// `domain::billing::ItemDisposition`'s doc comment.
fn validate_condition_disposition(
    condition: ItemCondition,
    disposition: Option<ItemDisposition>,
) -> AppResult<()> {
    match condition {
        ItemCondition::Damaged if disposition.is_none() => Err(AppError::validation(
            "A disposition (returnToSupplier, writeOffScrap, or repairPending) is required for a damaged item",
        )),
        ItemCondition::Damaged => Ok(()),
        _ if disposition.is_some() => Err(AppError::validation(
            "disposition must be omitted unless condition is 'damaged'",
        )),
        _ => Ok(()),
    }
}

/// Resolves a no-receipt line. With no invoice line to read the original
/// price from, a product line is valued at the price the cashier picked
/// (`sellingPriceCents`) and its units belong back in the newest batch of the
/// product ever sold at that price; a price no batch has ever had is refused
/// (`PRICE_TIER_UNAVAILABLE`) rather than valued or costed by guesswork. A
/// line with no `productKey` at all takes a client-supplied name/price.
async fn resolve_no_receipt_item(
    db: &Db,
    item_req: &CreateCreditNoteItemRequest,
) -> AppResult<CreditNoteItem> {
    validate_condition_disposition(item_req.condition, item_req.disposition)?;

    let (product_key, name, sku, unit_price_cents, source_type, batch_allocations) = match &item_req
        .product_key
    {
        Some(product_key) => {
            let product = inventory::service::product::get_product_by_key(db, product_key).await?;
            let price = item_req.selling_price_cents.ok_or_else(|| {
                AppError::validation(format!(
                    "Pick the price '{}' was sold at — sellingPriceCents is required",
                    product.name
                ))
            })?;
            let batch =
                inventory::service::stock_batch::newest_batch_at_price(db, &product.key, price)
                    .await?
                    .ok_or_else(|| {
                        AppError::conflict_with_details(
                            codes::PRICE_TIER_UNAVAILABLE,
                            format!("{} has never been sold at this price.", product.name),
                            serde_json::json!({ "sellingPriceCents": price }),
                        )
                    })?;
            (
                Some(product.key.clone()),
                product.name.clone(),
                Some(product.sku.clone()),
                price,
                Some("retail".to_string()),
                vec![BatchAllocation {
                    batch_key: batch.key,
                    quantity: item_req.quantity,
                    unit_cost_cents: batch
                        .unit_cost_cents
                        .expect("a batch read from the store carries its cost"),
                }],
            )
        }
        None => (
            None,
            item_req.name.clone().ok_or_else(|| {
                AppError::validation("A no-receipt item with no productKey must include a name")
            })?,
            None,
            item_req.unit_price_cents.ok_or_else(|| {
                AppError::validation(
                    "A no-receipt item with no productKey must include unitPriceCents",
                )
            })?,
            None,
            Vec::new(),
        ),
    };

    let total_cents = unit_price_cents * item_req.quantity;

    Ok(CreditNoteItem {
        product_key,
        name,
        sku,
        quantity: item_req.quantity,
        unit_price_cents,
        total_cents,
        reason: item_req.reason,
        condition: item_req.condition,
        disposition: item_req.disposition,
        serial_number: item_req.serial_number.clone(),
        within_warranty: None,
        notes: item_req.notes.clone(),
        source_type,
        source_ticket_key: None,
        source_ticket_number: None,
        batch_allocations,
    })
}

/// The refund/exchange computation shared by both `create_credit_note`
/// paths — pure computation over reads only (exchange-item/ticket
/// resolution, the invoice's existing payment legs for a proportional
/// split), no durable writes. Computed once per credit-note-creation
/// attempt so `refund_cash_cents`/`balance_reduction_cents` (which feed the
/// invoice's atomic quantity claim) are known *before* any stock movement,
/// payment, or credit-note document is written — see `create_credit_note`'s
/// doc comment for why that ordering matters.
#[derive(Debug)]
pub(super) struct RefundShape {
    resolved_exchange_items: Vec<InvoiceItem>,
    resolved_exchange_products: HashMap<String, Product>,
    exchange_subtotal_cents: i64,
    exchange_reference: Option<String>,
    net_refund_cents: i64,
    refund_cash_cents: i64,
    balance_reduction_cents: i64,
    refund_breakdown: Vec<RefundBreakdownLeg>,
}

#[allow(clippy::too_many_arguments)]
async fn compute_refund_shape(
    db: &Db,
    invoice_key: Option<&str>,
    return_subtotal_cents: i64,
    exchange_items_req: Option<&[CreateSaleItemRequest]>,
    payment_method: &str,
    explicit_refund_breakdown: Option<&[RefundBreakdownLeg]>,
    no_receipt: bool,
    already_paid_cents: i64,
) -> AppResult<RefundShape> {
    let (resolved_exchange_items, resolved_exchange_products) =
        if let Some(exchange_reqs) = exchange_items_req {
            if exchange_reqs.is_empty() {
                (Vec::new(), HashMap::new())
            } else {
                // An exchange replacement is resolved exactly like a sale line:
                // priced from the picked tier, units planned from its batches.
                super::super::sale::resolve_sale_items(db, exchange_reqs).await?
            }
        } else {
            (Vec::new(), HashMap::new())
        };
    let exchange_subtotal_cents: i64 = resolved_exchange_items
        .iter()
        .map(|item| item.total_cents)
        .sum();
    let exchange_reference = if resolved_exchange_items.is_empty() {
        None
    } else {
        Some(generate_id(prefixes::EXCHANGE))
    };

    let net_refund_cents = return_subtotal_cents - exchange_subtotal_cents;

    // The partial-payment cap only applies to a positive cashback against a
    // real invoice — a no-receipt return has no invoice to cap against, and
    // a negative net (customer pays extra) is never capped.
    let (refund_cash_cents, balance_reduction_cents) =
        calculations::refund_amounts(net_refund_cents, already_paid_cents, no_receipt);

    let refund_breakdown: Vec<RefundBreakdownLeg> = if refund_cash_cents == 0 {
        Vec::new()
    } else if let Some(explicit) = explicit_refund_breakdown {
        let sum: i64 = explicit.iter().map(|leg| leg.amount_cents).sum();
        if sum > refund_cash_cents {
            return Err(AppError::conflict_with_details(
                codes::REFUND_EXCEEDS_PAID_AMOUNT,
                format!(
                    "refundBreakdown sums to {sum} cents, which exceeds the refundable amount of {refund_cash_cents} cents"
                ),
                serde_json::json!({ "refundCashCents": refund_cash_cents, "breakdownSumCents": sum }),
            ));
        }
        if sum != refund_cash_cents {
            return Err(AppError::validation(format!(
                "refundBreakdown must sum to exactly the refundable amount ({refund_cash_cents} cents), got {sum} cents"
            )));
        }
        explicit.to_vec()
    } else if refund_cash_cents > 0 {
        // Split-paid invoices default to a proportional allocation across
        // the invoice's original payment legs; anything else is a single
        // leg using the resolved payment method.
        let invoice_legs: Vec<(String, i64)> = if let Some(invoice_key) = invoice_key {
            let split_payments = repository::list_payments_for_invoice(db, invoice_key).await?;
            split_payments
                .iter()
                .map(|p| (p.payment_method.clone(), p.amount_cents))
                .collect()
        } else {
            Vec::new()
        };
        calculations::default_refund_legs(refund_cash_cents, &invoice_legs, payment_method)
            .into_iter()
            .map(|(method, amount_cents)| RefundBreakdownLeg {
                method,
                amount_cents,
            })
            .collect()
    } else {
        vec![RefundBreakdownLeg {
            method: payment_method.to_string(),
            amount_cents: refund_cash_cents,
        }]
    };

    Ok(RefundShape {
        resolved_exchange_items,
        resolved_exchange_products,
        exchange_subtotal_cents,
        exchange_reference,
        net_refund_cents,
        refund_cash_cents,
        balance_reduction_cents,
        refund_breakdown,
    })
}

/// Applies the condition/disposition -> `StockMovementType` mapping for one
/// returned item. `Damaged`+`RepairPending` and `PendingInspection` write no
/// movement at all (parked — out of scope for this pass, see
/// `CreditNoteStatus::AwaitingResolution`'s doc comment).
async fn apply_return_stock_movement(
    db: &Db,
    item: &CreditNoteItem,
    credit_note_key: &str,
    credit_note_number: &str,
) {
    let Some(product_key) = &item.product_key else {
        return;
    };
    let movement_type = match (item.condition, item.disposition) {
        (ItemCondition::Resalable | ItemCondition::OpenBoxDiscount, _) => {
            Some((StockMovementType::ReturnRestock, item.quantity))
        }
        (ItemCondition::Damaged, Some(ItemDisposition::ReturnToSupplier)) => {
            Some((StockMovementType::ReturnSupplierRma, 0))
        }
        (ItemCondition::Damaged, Some(ItemDisposition::WriteOffScrap)) => {
            Some((StockMovementType::ReturnWriteOff, 0))
        }
        (ItemCondition::Damaged, Some(ItemDisposition::RepairPending)) => None,
        (ItemCondition::PendingInspection, _) => None,
        (ItemCondition::Damaged, None) => None, // unreachable — validated earlier
    };
    let Some((movement_type, delta)) = movement_type else {
        return;
    };
    let note = format!("Credit note {credit_note_number}");
    let result = if delta == 0 {
        // Audit-only: the unit leaves sellable stock for good (written off or
        // sent back to the supplier), so no batch is touched.
        match inventory::service::product::get_product_by_key(db, product_key).await {
            Ok(product) => match ObjectId::parse_str(&product.id) {
                Ok(product_object_id) => inventory::service::stock::apply_stock_delta(
                    db,
                    product_object_id,
                    None,
                    0,
                    movement_type,
                    Some(credit_note_key.to_string()),
                    Some(note),
                )
                .await
                .map(|_| ()),
                Err(_) => return,
            },
            Err(err) => Err(err),
        }
    } else {
        inventory::service::stock_batch::move_allocations(
            db,
            product_key,
            &item.batch_allocations,
            1,
            movement_type,
            credit_note_key,
            &note,
        )
        .await
    };
    if let Err(err) = result {
        tracing::warn!(product_key, error = %err, "could not apply credit note stock movement");
    }
}

/// Transitions a returned serialized unit's status per the condition/
/// disposition -> `SerialStatus` mapping: `Resalable`/`OpenBoxDiscount` ->
/// `ReturnedResalable`; `Damaged`+`ReturnToSupplier` -> `UnderWarrantyClaim`;
/// `Damaged`+`WriteOffScrap` -> `WrittenOff`; `Damaged`+`RepairPending` or
/// `PendingInspection` -> `ReturnedFaulty`. Best-effort, same as the stock
/// movement it accompanies — never fails the whole credit note.
async fn transition_returned_serial(db: &Db, item: &CreditNoteItem, credit_note_key: &str) {
    let (Some(product_key), Some(serial_number)) = (&item.product_key, &item.serial_number) else {
        return;
    };
    let new_status = match (item.condition, item.disposition) {
        (ItemCondition::Resalable | ItemCondition::OpenBoxDiscount, _) => {
            crate::domain::inventory::SerialStatus::ReturnedResalable
        }
        (ItemCondition::Damaged, Some(ItemDisposition::ReturnToSupplier)) => {
            crate::domain::inventory::SerialStatus::UnderWarrantyClaim
        }
        (ItemCondition::Damaged, Some(ItemDisposition::WriteOffScrap)) => {
            crate::domain::inventory::SerialStatus::WrittenOff
        }
        (ItemCondition::Damaged, Some(ItemDisposition::RepairPending))
        | (ItemCondition::PendingInspection, _) => {
            crate::domain::inventory::SerialStatus::ReturnedFaulty
        }
        (ItemCondition::Damaged, None) => return, // unreachable — validated earlier
    };

    let Ok(serial_id) =
        inventory::service::product_serial::find_serial_id(db, product_key, serial_number).await
    else {
        tracing::warn!(
            product_key,
            serial_number,
            "could not resolve returned serial"
        );
        return;
    };
    if let Err(err) = inventory::service::product_serial::transition_serial_on_return(
        db,
        serial_id,
        new_status,
        credit_note_key,
    )
    .await
    {
        tracing::warn!(product_key, serial_number, error = %err, "could not transition returned serial status");
    }
}

/// Cancels the warranties of what came back: each returned serial's, and a non-serialized
/// line's once all of it has been returned. Best-effort like the stock movement it follows; a
/// failure is logged, never undoes the credit note.
async fn void_returned_warranties(
    db: &Db,
    created: &CreditNote,
    actor: &warranties::service::WarrantyActor,
) {
    let Some(invoice_key) = created.invoice_key.as_deref() else {
        return;
    };
    for item in &created.returned_items {
        if let Some(serial_number) = &item.serial_number
            && let Err(err) = warranties::service::void_for_returned_serial(
                db,
                invoice_key,
                serial_number,
                &created.credit_note_number,
                actor,
            )
            .await
        {
            tracing::warn!(invoice_key, serial_number, error = %err, "could not cancel a returned unit's warranty");
        }
    }
    let invoice = match repository::find_invoice_by_id_or_key(db, invoice_key).await {
        Ok(Some(invoice)) => invoice,
        Ok(None) => return,
        Err(err) => {
            tracing::warn!(invoice_key, error = %err, "could not read the invoice to cancel returned warranties");
            return;
        }
    };
    for (line_index, line) in invoice.items.iter().enumerate() {
        if line.serial_numbers.is_empty()
            && line.returned_quantity >= line.quantity
            && let Err(err) = warranties::service::void_for_returned_line(
                db,
                invoice_key,
                line_index,
                &created.credit_note_number,
                actor,
            )
            .await
        {
            tracing::warn!(invoice_key, line_index, error = %err, "could not cancel a returned line's warranty");
        }
    }
}

/// Creates a credit note, processes stock adjustments, calculates
/// cashback/extra-payment (capped by what was actually paid, allocated
/// across payment methods for a split-paid invoice), and updates the
/// original invoice's `returned_quantity`/`refunded_cents`/
/// `credit_note_count`. No-receipt and past-return-window requests require
/// the caller to hold `Role::Admin`.
///
/// For the normal (invoice-matched) path, claiming the returned quantity
/// against the invoice is the *first* durable write, atomic and
/// version-guarded (`update_invoice_credit_note_progress`'s `expected_version`
/// filter) — deliberately ordered before stock movements, payment inserts,
/// sequence reservation, and the credit-note document itself. Without this,
/// two credit-note requests moments apart could both read the same
/// pre-return `returnedQuantity`, both pass validation, and the second's
/// write would silently clobber the first's increment while `refundedCents`/
/// `creditNoteCount` (real atomic `$inc`s) both still accumulated — the
/// exact shape of a real incident (two credit notes on one invoice summing
/// to more than its total). Claiming first, with a version-guarded retry
/// loop on conflict, means nothing else is ever written for a request that
/// ultimately can't be satisfied, and no concurrent request can double-claim
/// the same quantity.
pub async fn create_credit_note(
    db: &Db,
    config: &Config,
    body: CreateCreditNoteRequest,
    cashier_id: String,
    cashier_name: String,
    caller_role: Option<Role>,
    device_id: Option<String>,
) -> AppResult<CreditNote> {
    crate::core::logging::domain::tracked("billing.credit_note_created", async move {
        if body.returned_items.is_empty() {
            return Err(AppError::validation("At least one item must be returned"));
        }
        for item_req in &body.returned_items {
            if item_req.quantity < 1 {
                return Err(AppError::validation(
                    "Return item quantity must be at least 1",
                ));
            }
            validate_condition_disposition(item_req.condition, item_req.disposition)?;
        }
        if body.no_receipt && body.warranty_claim_key.is_some() {
            return Err(AppError::validation(
                "A warranty refund is made against the invoice the item was sold on",
            ));
        }
        if body.no_receipt && body.invoice_key.is_some() {
            return Err(AppError::validation(
                "A no-receipt credit note cannot also reference an invoiceKey",
            ));
        }
        if !body.no_receipt && body.invoice_key.is_none() {
            return Err(AppError::validation(
                "invoiceKey is required unless noReceipt is true",
            ));
        }

        let is_admin = caller_role == Some(Role::Admin);
        let has_override_reason = body
            .override_reason
            .as_ref()
            .is_some_and(|r| !r.trim().is_empty());

        // ---- No-receipt path: no invoice to match against, current-price valuation ----
        if body.no_receipt {
            if !has_override_reason {
                return Err(AppError::validation(
                    "overrideReason is required for a no-receipt credit note",
                ));
            }
            if !is_admin {
                return Err(AppError::forbidden_with_code(
                    "A manager must approve a no-receipt return",
                    codes::MANAGER_OVERRIDE_REQUIRED,
                ));
            }

            let mut resolved_return_items = Vec::new();
            let mut return_subtotal_cents: i64 = 0;
            for item_req in &body.returned_items {
                let resolved = resolve_no_receipt_item(db, item_req).await?;
                return_subtotal_cents += resolved.total_cents;
                resolved_return_items.push(resolved);
            }

            let refund_shape = compute_refund_shape(
                db,
                None,
                return_subtotal_cents,
                body.exchange_items.as_deref(),
                body.payment_method.as_deref().unwrap_or("cash"),
                body.refund_breakdown.as_deref(),
                true,
                0,
            )
            .await?;

            return finish_create_credit_note(
                db,
                None,
                resolved_return_items,
                return_subtotal_cents,
                refund_shape,
                true,
                true,
                Some(cashier_id.clone()),
                body.override_reason,
                body.notes,
                body.session_key,
                cashier_id,
                cashier_name,
                device_id,
            )
            .await;
        }

        // ---- Normal path: matched against an existing invoice ----
        // Claim the returned quantity against the invoice FIRST, atomically and
        // version-guarded, before any other durable write (stock movements,
        // payment inserts, sequence reservation, the credit-note document
        // itself) — see this function's doc comment for why. A version conflict
        // (a concurrent write already bumped the invoice) re-fetches and
        // re-validates from scratch and retries, bounded to a few attempts.
        let invoice_key = body.invoice_key.as_ref().expect("checked above").clone();
        let warranty_claim_key = body.warranty_claim_key.clone();
        if let Some(claim_key) = &warranty_claim_key {
            let returned_serials: Vec<&str> = body
                .returned_items
                .iter()
                .filter_map(|item| item.serial_number.as_deref())
                .collect();
            warranties::service::check_claim_refund(db, claim_key, &invoice_key, &returned_serials)
                .await?;
        }
        const MAX_CLAIM_ATTEMPTS: u32 = 3;
        let mut attempt = 0;

        let (
            invoice_id,
            invoice_key_owned,
            invoice_number,
            customer_key,
            resolved_return_items,
            return_subtotal_cents,
            refund_shape,
            is_manager_override,
        ) = loop {
            attempt += 1;

            let invoice = repository::find_invoice_by_id_or_key(db, &invoice_key)
                .await?
                .ok_or_else(|| {
                    AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
                })?;

            if invoice.status == InvoiceStatus::Voided {
                return Err(AppError::conflict(
                    codes::INVOICE_NOT_ELIGIBLE_FOR_CREDIT_NOTE,
                    "A voided invoice cannot have a credit note created against it",
                ));
            }

            let days_since_sale = (chrono::Utc::now() - invoice.created_at.to_chrono()).num_days();
            let mut is_manager_override = false;
            if days_since_sale > config.return_window_days && warranty_claim_key.is_none() {
                if !has_override_reason {
                    return Err(AppError::conflict(
                        codes::RETURN_WINDOW_EXPIRED,
                        format!(
                            "This sale was made {days_since_sale} days ago, outside the {}-day return window",
                            config.return_window_days
                        ),
                    ));
                }
                if !is_admin {
                    return Err(AppError::forbidden_with_code(
                        "A manager must approve a return outside the normal return window",
                        codes::MANAGER_OVERRIDE_REQUIRED,
                    ));
                }
                is_manager_override = true;
            }

            let mut updated_invoice_items = invoice.items.clone();
            let mut resolved_return_items = Vec::new();
            let mut return_subtotal_cents: i64 = 0;

            for item_req in &body.returned_items {
                let matched_idx = line_match(
                    &updated_invoice_items,
                    item_req.product_key.as_deref(),
                    item_req.source_ticket_key.as_deref(),
                    item_req.name.as_deref(),
                    LineHint {
                        unit_price_cents: item_req.selling_price_cents,
                        batch_keys: &[],
                    },
                    updated_invoice_items.len() == 1 && body.returned_items.len() == 1,
                )
                    .ok_or_else(|| {
                        AppError::validation_with_code(
                            format!(
                                "Item '{}' was not found on invoice {}",
                                item_req.name.as_deref().unwrap_or("unknown"),
                                invoice.invoice_number
                            ),
                            codes::VALIDATION_ERROR,
                        )
                    })?;

                let inv_item = &mut updated_invoice_items[matched_idx];
                let available_qty = inv_item.quantity - inv_item.returned_quantity;

                if item_req.quantity > available_qty {
                    return Err(AppError::custom(
                        axum::http::StatusCode::BAD_REQUEST,
                        codes::CREDIT_NOTE_QUANTITY_EXCEEDED,
                        format!(
                            "Return quantity ({}) exceeds available returnable quantity ({}) for item '{}'",
                            item_req.quantity, available_qty, inv_item.name
                        ),
                    ));
                }

                let unit_price_cents = calculations::returned_unit_price_cents(
                    inv_item.total_cents,
                    inv_item.quantity,
                    inv_item.unit_price_cents,
                );
                let line_total_cents = unit_price_cents * item_req.quantity;
                return_subtotal_cents += line_total_cents;
                // Units go back into the batches this line sold them from,
                // continuing past what earlier returns already put back.
                let mut batch_allocations =
                    inventory::service::stock_batch::allocations_for_return(
                        &inv_item.batch_allocations,
                        inv_item.returned_quantity,
                        item_req.quantity,
                    );
                inv_item.returned_quantity += item_req.quantity;

                // A serialized invoice line must return a specific unit — resolve
                // it against the sold serial before this credit note is created
                // (fail before any write, same D4-style rule sale resolution uses)
                // and compute `withinWarranty` from it for the response/document.
                let within_warranty = if !inv_item.serial_numbers.is_empty() {
                    let serial_number = item_req.serial_number.as_deref().ok_or_else(|| {
                        AppError::validation(format!(
                            "'{}' is a serialized item — serialNumber is required",
                            inv_item.name
                        ))
                    })?;
                    let product_key = inv_item.product_key.as_deref().ok_or_else(|| {
                        AppError::validation(format!(
                            "'{}' has no productKey and cannot be resolved as a serialized item",
                            inv_item.name
                        ))
                    })?;
                    let serial = inventory::service::product_serial::resolve_sold_serial_for_invoice(
                        db,
                        product_key,
                        serial_number,
                        &invoice.key,
                    )
                    .await?;
                    // A specific unit goes back into its own batch.
                    let unit_cost_cents = inv_item
                        .batch_allocations
                        .iter()
                        .find(|a| a.batch_key == serial.batch_key)
                        .map(|a| a.unit_cost_cents)
                        .ok_or_else(|| {
                            AppError::validation(format!(
                                "Serial '{serial_number}' was not sold from this invoice line"
                            ))
                        })?;
                    batch_allocations = vec![BatchAllocation {
                        batch_key: serial.batch_key.clone(),
                        quantity: item_req.quantity,
                        unit_cost_cents,
                    }];
                    warranties::service::serial_within_warranty(db, &invoice.key, serial_number)
                        .await?
                } else {
                    None
                };

                resolved_return_items.push(CreditNoteItem {
                    product_key: inv_item.product_key.clone(),
                    name: inv_item.name.clone(),
                    sku: inv_item.sku.clone(),
                    quantity: item_req.quantity,
                    unit_price_cents,
                    total_cents: line_total_cents,
                    reason: item_req.reason,
                    condition: item_req.condition,
                    disposition: item_req.disposition,
                    serial_number: item_req.serial_number.clone(),
                    within_warranty,
                    notes: item_req.notes.clone(),
                    source_type: Some(inv_item.source_type.clone()),
                    source_ticket_key: inv_item.source_ticket_key.clone(),
                    source_ticket_number: inv_item.source_ticket_number.clone(),
                    batch_allocations,
                });
            }

            let already_paid_cents: i64 = repository::list_payments_for_invoice(db, &invoice.key)
                .await?
                .iter()
                .map(|p| p.amount_cents)
                .sum();

            // Pure computation, no durable writes — resolves exchange items,
            // the partial-payment refund cap, and the refund-method allocation,
            // so the exact `refunded_cents` delta to claim is known before the
            // atomic claim below.
            let refund_shape = compute_refund_shape(
                db,
                Some(&invoice.key),
                return_subtotal_cents,
                body.exchange_items.as_deref(),
                body.payment_method
                    .as_deref()
                    .unwrap_or(&invoice.payment_method),
                body.refund_breakdown.as_deref(),
                false,
                already_paid_cents,
            )
            .await?;

            let refunded_cents_delta =
                refund_shape.refund_cash_cents.max(0) + refund_shape.balance_reduction_cents.max(0);
            let invoice_id = invoice
                .id
                .expect("persisted invoice document must have an _id");

            match repository::invoice::update_invoice_credit_note_progress(
                db,
                invoice_id,
                updated_invoice_items,
                refunded_cents_delta,
                1,
                invoice.version,
            )
            .await?
            {
                Some(_) => {
                    break (
                        invoice_id,
                        invoice.key.clone(),
                        invoice.invoice_number.clone(),
                        invoice.customer_key.clone(),
                        resolved_return_items,
                        return_subtotal_cents,
                        refund_shape,
                        is_manager_override,
                    );
                }
                None => {
                    if attempt >= MAX_CLAIM_ATTEMPTS {
                        return Err(AppError::conflict(
                            codes::CREDIT_NOTE_CONTENTION,
                            "This invoice was updated by another request just now — please try again",
                        ));
                    }
                    // Version mismatch: a concurrent write already claimed
                    // against this invoice. Loop back and re-fetch/re-validate
                    // fresh rather than retrying with stale data.
                }
            }
        };

        // The quantity claim is now durably committed — nothing else has been
        // written yet. Proceed to the remaining side effects (sequence
        // reservation, stock movements, payment inserts, the credit-note
        // document itself).
        let warranty_actor = warranties::service::WarrantyActor {
            user_id: cashier_id.clone(),
            name: cashier_name.clone(),
        };
        let result = finish_create_credit_note(
            db,
            Some((invoice_key_owned, invoice_number)),
            resolved_return_items,
            return_subtotal_cents,
            refund_shape,
            is_manager_override,
            false,
            if is_manager_override {
                Some(cashier_id.clone())
            } else {
                None
            },
            body.override_reason,
            body.notes,
            body.session_key,
            cashier_id,
            cashier_name,
            device_id,
        )
        .await;

        match &result {
            Ok(created) => {
                void_returned_warranties(db, created, &warranty_actor).await;
                if let Some(claim_key) = &warranty_claim_key
                    && let Err(err) = warranties::service::finish_claim_refund(
                        db,
                        claim_key,
                        &created.key,
                        &created.credit_note_number,
                        &warranty_actor,
                    )
                    .await
                {
                    tracing::error!(claim_key, credit_note = %created.credit_note_number, error = %err, "credit note written but its warranty claim was not closed");
                }
                if let Some(customer_key) = &customer_key
                    && created.refund_cash_cents != 0
                    && let Err(err) = customers::service::apply_financial_delta(
                        db,
                        customer_key,
                        -created.refund_cash_cents,
                        0,
                    )
                    .await
                {
                    tracing::warn!(customer_key, error = %err, "could not adjust customer balance on credit note");
                }
            }
            Err(err) => {
                // The invoice's returned_quantity/refunded_cents/credit_note_count
                // claim already committed above (by design — see this function's
                // doc comment) but the credit note document itself failed to
                // materialize. Rare (only non-validation failures reach here,
                // e.g. a transient DB error during sequence reservation or the
                // document insert) but worth surfacing loudly: the invoice now
                // shows quantity claimed with no corresponding credit note.
                tracing::error!(
                    invoice_id = %invoice_id,
                    error = %err,
                    "invoice quantity claimed but credit note creation failed after the claim"
                );
            }
        }

        result
    })
    .await
}

/// Shared tail of `create_credit_note`'s no-receipt and normal paths: takes
/// an already-computed `RefundShape` (see `compute_refund_shape` — a pure,
/// no-writes step run *before* this function, so the normal path can claim
/// the invoice's returned quantity atomically before any of this function's
/// durable writes happen) and writes the stock movements, records the
/// refund/extra-payment leg(s), and persists the `CreditNoteDocument`.
#[allow(clippy::too_many_arguments)]
async fn finish_create_credit_note(
    db: &Db,
    invoice_ref: Option<(String, String)>,
    resolved_return_items: Vec<CreditNoteItem>,
    return_subtotal_cents: i64,
    refund_shape: RefundShape,
    is_manager_override: bool,
    no_receipt: bool,
    override_approved_by: Option<String>,
    override_reason: Option<String>,
    notes: Option<String>,
    session_key: Option<String>,
    cashier_id: String,
    cashier_name: String,
    device_id: Option<String>,
) -> AppResult<CreditNote> {
    let RefundShape {
        resolved_exchange_items,
        resolved_exchange_products,
        exchange_subtotal_cents,
        exchange_reference,
        net_refund_cents,
        refund_cash_cents,
        balance_reduction_cents,
        refund_breakdown,
    } = refund_shape;

    let reservation = sequences::service::reserve_sequence(
        db,
        "creditNote".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id: device_id.clone(),
        },
    )
    .await?;
    let credit_note_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );
    let credit_note_key = generate_id(prefixes::CREDIT_NOTE);

    for ret_item in &resolved_return_items {
        apply_return_stock_movement(db, ret_item, &credit_note_key, &credit_note_number).await;
        transition_returned_serial(db, ret_item, &credit_note_key).await;
    }
    for ex_item in &resolved_exchange_items {
        if ex_item.source_type == "retail"
            && let Some(product_key) = &ex_item.product_key
            && resolved_exchange_products.contains_key(product_key)
            && let Err(err) = inventory::service::stock_batch::move_allocations(
                db,
                product_key,
                &ex_item.batch_allocations,
                -1,
                StockMovementType::Sale,
                &credit_note_key,
                &format!("Exchange replacement on credit note {credit_note_number}"),
            )
            .await
        {
            tracing::warn!(product_key, error = %err, "could not deduct stock for exchange product");
        }
    }

    let resolved_session_doc = crate::modules::sessions::service::resolve_writable_session(
        db,
        session_key.as_deref(),
        &cashier_id,
    )
    .await?;
    let resolved_session_key = resolved_session_doc.as_ref().map(|s| s.key.clone());

    let now = BsonDateTime::now();
    let mut refund_payment_keys = Vec::new();
    if refund_cash_cents != 0 {
        for leg in &refund_breakdown {
            let payment_doc = PaymentDocument {
                id: None,
                key: generate_id(prefixes::PAYMENT),
                invoice_key: invoice_ref
                    .as_ref()
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default(),
                amount_cents: -leg.amount_cents,
                payment_method: leg.method.clone(),
                notes: Some(if refund_cash_cents > 0 {
                    format!("Refund for credit note {credit_note_number}")
                } else {
                    format!("Payment for exchange/credit note difference {credit_note_number}")
                }),
                session_key: resolved_session_key.clone(),
                recorded_by_user_id: cashier_id.clone(),
                recorded_by_name_snapshot: cashier_name.clone(),
                recorded_at: now,
                version: 1,
                created_at: now,
                updated_at: now,
            };
            if invoice_ref.is_some()
                && let Ok(inserted) = repository::insert_payment(db, payment_doc).await
            {
                refund_payment_keys.push(inserted.key);
            }
        }

        if refund_cash_cents > 0 {
            let cash_refunded: i64 = refund_breakdown
                .iter()
                .filter(|l| l.method == "cash")
                .map(|l| l.amount_cents)
                .sum();
            if cash_refunded > 0
                && let Some(ref s_key) = resolved_session_key
            {
                crate::modules::sessions::repository::sessions::increment_session_refunds(
                    db,
                    s_key,
                    cash_refunded,
                )
                .await?;
                let drawer_key = crate::modules::sessions::service::active_drawer_key(
                    db,
                    resolved_session_doc.as_ref(),
                    &cashier_id,
                )
                .await?;
                crate::modules::sessions::repository::drawer::update_drawer_balance(
                    db,
                    &drawer_key,
                    -cash_refunded,
                )
                .await?;
            }
        }
    }

    let status = if resolved_return_items
        .iter()
        .any(|item| item.condition == ItemCondition::PendingInspection)
    {
        CreditNoteStatus::AwaitingResolution
    } else {
        CreditNoteStatus::Resolved
    };

    let document = CreditNoteDocument {
        id: None,
        key: credit_note_key,
        credit_note_number,
        invoice_key: invoice_ref.as_ref().map(|(k, _)| k.clone()),
        invoice_number: invoice_ref.as_ref().map(|(_, n)| n.clone()),
        no_receipt,
        customer_key: None,
        customer_name_snapshot: None,
        cashier_id,
        cashier_name_snapshot: cashier_name,
        returned_items: resolved_return_items,
        exchange_items: resolved_exchange_items,
        exchange_reference,
        return_subtotal_cents,
        exchange_subtotal_cents,
        net_refund_cents,
        refund_cash_cents,
        balance_reduction_cents,
        refund_breakdown,
        refund_payment_keys,
        status,
        is_manager_override,
        override_approved_by,
        override_reason,
        notes,
        session_key: resolved_session_key,
        voided_at: None,
        voided_by: None,
        voided_reason: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let inserted = repository::credit_notes::insert_credit_note(db, document).await?;
    Ok(inserted.into_credit_note())
}
