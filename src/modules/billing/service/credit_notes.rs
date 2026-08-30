// Business rules and orchestration for credit notes: returns, refunds
// (cashback), and replacements/exchanges. Gated by `CurrentUser` +
// `billing:write` permission (return-window override and no-receipt returns
// additionally require the caller to hold `Role::Admin`). A credit note is
// created once as a single atomic write — this codebase has no edit-in-place
// endpoint for any billing document, so there is deliberately no way to move
// one out of `AwaitingResolution` other than `void_credit_note`.

use std::collections::HashMap;

use futures_util::future::try_join_all;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::{
        config::Config,
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::{build_bson_regex, calculate_pagination, today_utc_range},
    },
    domain::{
        billing::{
            CreateCreditNoteItemRequest, CreateCreditNoteRequest, CreateSaleItemRequest,
            CreditNote, CreditNoteItem, CreditNoteListQuery, CreditNoteListResponse,
            CreditNoteStatus, InvoiceItem, InvoiceStatus, ItemCondition, ItemDisposition,
            RefundBreakdownLeg, VoidCreditNoteRequest,
        },
        inventory::{Product, StockMovementType},
        sequences::ReserveSequenceRequest,
        users::Role,
    },
    modules::{
        billing::{
            model::{CreditNoteDocument, PaymentDocument},
            repository,
        },
        customers, inventory, print_jobs, repairs, sequences,
    },
};

/// Resolves exchange sale items from catalog / tickets — unchanged from the
/// pre-Credit-Note `returns` module: an exchange replacement item is
/// resolved exactly like a normal sale line.
async fn resolve_exchange_items(
    db: &Database,
    items: &[CreateSaleItemRequest],
) -> AppResult<(Vec<InvoiceItem>, HashMap<String, Product>)> {
    let retail_product_keys: Vec<String> = items
        .iter()
        .filter(|item| item.source_type == "retail")
        .filter_map(|item| item.product_key.clone())
        .collect();
    let products: HashMap<String, Product> =
        inventory::service::product::get_products_by_keys(db, &retail_product_keys)
            .await?
            .into_iter()
            .map(|product| (product.key.clone(), product))
            .collect();

    let resolved: Vec<InvoiceItem> = try_join_all(
        items
            .iter()
            .map(|item| resolve_exchange_item(db, item, &products)),
    )
    .await?;

    Ok((resolved, products))
}

async fn resolve_exchange_item(
    db: &Database,
    item: &CreateSaleItemRequest,
    products: &HashMap<String, Product>,
) -> AppResult<InvoiceItem> {
    let (
        name,
        sku,
        unit_price_cents,
        source_ticket_number,
        assigned_employee_name,
        unit_cost_cents,
    ) = match item.source_type.as_str() {
        "retail" => match &item.product_key {
            Some(product_key) => {
                let product = products.get(product_key).ok_or_else(|| {
                    AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
                })?;
                (
                    product.name.clone(),
                    Some(product.sku.clone()),
                    product.selling_price_cents,
                    None,
                    None,
                    Some(product.cost_price_cents),
                )
            }
            None => (
                item.name.clone().ok_or_else(|| {
                    AppError::validation("An item with no productKey must include a name")
                })?,
                None,
                item.unit_price_cents.ok_or_else(|| {
                    AppError::validation("An item with no productKey must include unitPriceCents")
                })?,
                None,
                None,
                None,
            ),
        },
        "repair" => match &item.source_ticket_key {
            Some(ticket_key) => {
                let ticket = repairs::service::get_repair(db, ticket_key).await?;
                let estimated_cost_cents = ticket.estimated_cost_cents.ok_or_else(|| {
                    AppError::validation_with_code(
                        format!(
                            "Repair ticket {} has no price yet, set a repair price before billing it",
                            ticket.ticket_number
                        ),
                        codes::REPAIR_PRICE_REQUIRED,
                    )
                })?;
                (
                    item.name
                        .clone()
                        .unwrap_or_else(|| ticket.device_model.clone()),
                    None,
                    estimated_cost_cents + ticket.material_cost_cents.unwrap_or(0),
                    Some(ticket.ticket_number),
                    ticket.assigned_employee_name,
                    Some(ticket.material_cost_cents.unwrap_or(0) / item.quantity.max(1)),
                )
            }
            None => (
                item.name.clone().ok_or_else(|| {
                    AppError::validation(
                        "A repair item with no sourceTicketKey must include a name",
                    )
                })?,
                None,
                item.unit_price_cents.ok_or_else(|| {
                    AppError::validation(
                        "A repair item with no sourceTicketKey must include unitPriceCents",
                    )
                })?,
                None,
                item.assigned_employee_name.clone(),
                None,
            ),
        },
        "print" => match &item.source_ticket_key {
            Some(ticket_key) => {
                let ticket = print_jobs::service::get_print_job(db, ticket_key).await?;
                (
                    item.name.clone().unwrap_or_else(|| ticket.job_type.clone()),
                    None,
                    ticket.estimated_cost_cents + ticket.material_cost_cents.unwrap_or(0),
                    Some(ticket.ticket_number),
                    ticket.assigned_employee_name,
                    Some(ticket.material_cost_cents.unwrap_or(0) / item.quantity.max(1)),
                )
            }
            None => (
                item.name.clone().ok_or_else(|| {
                    AppError::validation("A print item with no sourceTicketKey must include a name")
                })?,
                None,
                item.unit_price_cents.ok_or_else(|| {
                    AppError::validation(
                        "A print item with no sourceTicketKey must include unitPriceCents",
                    )
                })?,
                None,
                item.assigned_employee_name.clone(),
                None,
            ),
        },
        other => {
            return Err(AppError::validation(format!(
                "Invalid item sourceType '{other}'"
            )));
        }
    };

    let total_cents = (unit_price_cents * item.quantity - item.discount_cents).max(0);

    Ok(InvoiceItem {
        product_key: item.product_key.clone(),
        name,
        sku,
        unit_price_cents,
        quantity: item.quantity,
        discount_cents: item.discount_cents,
        total_cents,
        unit_cost_cents,
        source_type: item.source_type.clone(),
        source_ticket_key: item.source_ticket_key.clone(),
        source_ticket_number,
        assigned_employee_name,
        returned_quantity: 0,
        // Exchange replacement items don't support serialized-unit
        // selection in this pass — a serialized product taken as a
        // replacement in an exchange is out of scope; add it here if a
        // future change extends `CreateSaleItemRequest`-shaped exchange
        // items with `serial_numbers` the same way a plain sale line has.
        serial_numbers: Vec::new(),
    })
}

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

/// Resolves a no-receipt line: valued at the product's *current* selling
/// price (there is no invoice line to read an original price from), or a
/// client-supplied name/price for a line with no `productKey` at all.
async fn resolve_no_receipt_item(
    db: &Database,
    item_req: &CreateCreditNoteItemRequest,
) -> AppResult<CreditNoteItem> {
    validate_condition_disposition(item_req.condition, item_req.disposition)?;

    let (product_key, name, sku, unit_price_cents, source_type) = match &item_req.product_key {
        Some(product_key) => {
            let product = inventory::service::product::get_product_by_key(db, product_key).await?;
            (
                Some(product.key.clone()),
                product.name.clone(),
                Some(product.sku.clone()),
                product.selling_price_cents,
                Some("retail".to_string()),
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
    })
}

/// Splits `total` across `weights` proportionally, rounding down per leg and
/// handing the last leg whatever remainder rounding left over — guarantees
/// the returned amounts sum to exactly `total` (required so a generated
/// `refund_breakdown` always reconciles to `refund_cash_cents`).
fn proportional_split(total: i64, weights: &[i64]) -> Vec<i64> {
    let weight_sum: i64 = weights.iter().sum();
    if weight_sum <= 0 || weights.is_empty() {
        return vec![total];
    }
    let mut amounts: Vec<i64> = weights
        .iter()
        .map(|w| (total as i128 * *w as i128 / weight_sum as i128) as i64)
        .collect();
    let allocated: i64 = amounts.iter().sum();
    let remainder = total - allocated;
    if let Some(last) = amounts.last_mut() {
        *last += remainder;
    }
    amounts
}

/// The refund/exchange computation shared by both `create_credit_note`
/// paths — pure computation over reads only (exchange-item/ticket
/// resolution, the invoice's existing payment legs for a proportional
/// split), no durable writes. Computed once per credit-note-creation
/// attempt so `refund_cash_cents`/`balance_reduction_cents` (which feed the
/// invoice's atomic quantity claim) are known *before* any stock movement,
/// payment, or credit-note document is written — see `create_credit_note`'s
/// doc comment for why that ordering matters.
struct RefundShape {
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
    db: &Database,
    invoice_key: Option<&str>,
    return_subtotal_cents: i64,
    exchange_items_req: &Option<Vec<CreateSaleItemRequest>>,
    payment_method: &str,
    explicit_refund_breakdown: &Option<Vec<RefundBreakdownLeg>>,
    no_receipt: bool,
    already_paid_cents: i64,
) -> AppResult<RefundShape> {
    let (resolved_exchange_items, resolved_exchange_products) =
        if let Some(exchange_reqs) = exchange_items_req {
            if exchange_reqs.is_empty() {
                (Vec::new(), HashMap::new())
            } else {
                resolve_exchange_items(db, exchange_reqs).await?
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
    let (refund_cash_cents, balance_reduction_cents) = if net_refund_cents > 0 && !no_receipt {
        let cash = net_refund_cents.min(already_paid_cents.max(0));
        (cash, net_refund_cents - cash)
    } else {
        (net_refund_cents, 0)
    };

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
        explicit.clone()
    } else if refund_cash_cents > 0 {
        // Split-paid invoices default to a proportional allocation across
        // the invoice's original payment legs; anything else is a single
        // leg using the resolved payment method.
        let split_payments = repository::list_payments_for_invoice(db, invoice_key.unwrap_or(""))
            .await
            .unwrap_or_default();
        let positive_legs: Vec<&PaymentDocument> = split_payments
            .iter()
            .filter(|p| p.amount_cents > 0)
            .collect();
        if invoice_key.is_some() && positive_legs.len() > 1 {
            let weights: Vec<i64> = positive_legs.iter().map(|p| p.amount_cents).collect();
            let amounts = proportional_split(refund_cash_cents, &weights);
            positive_legs
                .iter()
                .zip(amounts)
                .map(|(leg, amount_cents)| RefundBreakdownLeg {
                    method: leg.payment_method.clone(),
                    amount_cents,
                })
                .collect()
        } else {
            vec![RefundBreakdownLeg {
                method: payment_method.to_string(),
                amount_cents: refund_cash_cents,
            }]
        }
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
    db: &Database,
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
    let Ok(product) = inventory::service::product::get_product_by_key(db, product_key).await else {
        tracing::warn!(
            product_key,
            "could not resolve product for credit note stock movement"
        );
        return;
    };
    let Ok(product_object_id) = ObjectId::parse_str(&product.id) else {
        return;
    };
    if let Err(err) = inventory::service::stock::apply_stock_delta(
        db,
        product_object_id,
        delta,
        movement_type,
        Some(credit_note_key.to_string()),
        Some(format!("Credit note {credit_note_number}")),
    )
    .await
    {
        tracing::warn!(product_key, error = %err, "could not apply credit note stock movement");
    }
}

/// Transitions a returned serialized unit's status per the condition/
/// disposition -> `SerialStatus` mapping: `Resalable`/`OpenBoxDiscount` ->
/// `ReturnedResalable`; `Damaged`+`ReturnToSupplier` -> `UnderWarrantyClaim`;
/// `Damaged`+`WriteOffScrap` -> `WrittenOff`; `Damaged`+`RepairPending` or
/// `PendingInspection` -> `ReturnedFaulty`. Best-effort, same as the stock
/// movement it accompanies — never fails the whole credit note.
async fn transition_returned_serial(db: &Database, item: &CreditNoteItem, credit_note_key: &str) {
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
    db: &Database,
    config: &Config,
    body: CreateCreditNoteRequest,
    cashier_id: String,
    cashier_name: String,
    caller_role: Option<Role>,
    device_id: Option<String>,
) -> AppResult<CreditNote> {
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
            &body.exchange_items,
            body.payment_method.as_deref().unwrap_or("cash"),
            &body.refund_breakdown,
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
        if days_since_sale > config.return_window_days {
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
            let matched_idx = updated_invoice_items
                .iter()
                .position(|item| {
                    if let (Some(req_pk), Some(item_pk)) =
                        (&item_req.product_key, &item.product_key)
                        && req_pk == item_pk
                    {
                        return true;
                    }
                    if let (Some(req_stk), Some(item_stk)) =
                        (&item_req.source_ticket_key, &item.source_ticket_key)
                        && req_stk == item_stk
                    {
                        return true;
                    }
                    if let Some(req_name) = &item_req.name
                        && req_name.eq_ignore_ascii_case(&item.name)
                    {
                        return true;
                    }
                    if updated_invoice_items.len() == 1 && body.returned_items.len() == 1 {
                        return true;
                    }
                    false
                })
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

            let unit_price_cents = if inv_item.quantity > 0 {
                inv_item.total_cents / inv_item.quantity
            } else {
                inv_item.unit_price_cents
            };
            let line_total_cents = unit_price_cents * item_req.quantity;
            return_subtotal_cents += line_total_cents;
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
                inventory::service::product_serial::is_within_warranty(&serial)
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
            &body.exchange_items,
            body.payment_method
                .as_deref()
                .unwrap_or(&invoice.payment_method),
            &body.refund_breakdown,
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
        cashier_id,
        cashier_name,
        device_id,
    )
    .await;

    match &result {
        Ok(created) => {
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
}

/// Shared tail of `create_credit_note`'s no-receipt and normal paths: takes
/// an already-computed `RefundShape` (see `compute_refund_shape` — a pure,
/// no-writes step run *before* this function, so the normal path can claim
/// the invoice's returned quantity atomically before any of this function's
/// durable writes happen) and writes the stock movements, records the
/// refund/extra-payment leg(s), and persists the `CreditNoteDocument`.
#[allow(clippy::too_many_arguments)]
async fn finish_create_credit_note(
    db: &Database,
    invoice_ref: Option<(String, String)>,
    resolved_return_items: Vec<CreditNoteItem>,
    return_subtotal_cents: i64,
    refund_shape: RefundShape,
    is_manager_override: bool,
    no_receipt: bool,
    override_approved_by: Option<String>,
    override_reason: Option<String>,
    notes: Option<String>,
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
            device_id,
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
            && let Some(product) = resolved_exchange_products.get(product_key)
            && let Ok(product_object_id) = ObjectId::parse_str(&product.id)
            && let Err(err) = inventory::service::stock::apply_stock_delta(
                db,
                product_object_id,
                -ex_item.quantity,
                StockMovementType::Sale,
                Some(credit_note_key.clone()),
                Some(format!(
                    "Exchange replacement on credit note {credit_note_number}"
                )),
            )
            .await
        {
            tracing::warn!(product_key, error = %err, "could not deduct stock for exchange product");
        }
    }

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

/// Reverses a credit note's stock/payment/customer-balance effects and marks
/// it `Voided` with a mandatory reason — mirrors `sale::void_invoice`'s
/// posture. Reverses `ReturnRestock` movements (deducts the quantity back
/// out) but does not attempt to reverse `ReturnWriteOff`/`ReturnSupplierRma`
/// (audit-only, no quantity change to undo) or re-credit exchange stock
/// (out of scope for this pass — voiding an exchange's replacement item sale
/// is not a returns flow).
pub async fn void_credit_note(
    db: &Database,
    id_or_key: &str,
    body: VoidCreditNoteRequest,
    voided_by: String,
) -> AppResult<CreditNote> {
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
        if let Ok(product) = inventory::service::product::get_product_by_key(db, product_key).await
            && let Ok(product_object_id) = ObjectId::parse_str(&product.id)
            && let Err(err) = inventory::service::stock::apply_stock_delta(
                db,
                product_object_id,
                -item.quantity,
                StockMovementType::ReturnRestock,
                Some(existing.key.clone()),
                Some(format!(
                    "Void of credit note {}",
                    existing.credit_note_number
                )),
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
        let restored_items: Vec<InvoiceItem> = invoice
            .items
            .into_iter()
            .map(|mut inv_item| {
                if let Some(returned) = existing.returned_items.iter().find(|ri| {
                    ri.product_key == inv_item.product_key
                        && ri.source_ticket_key == inv_item.source_ticket_key
                        && ri.name.eq_ignore_ascii_case(&inv_item.name)
                }) {
                    inv_item.returned_quantity =
                        (inv_item.returned_quantity - returned.quantity).max(0);
                }
                inv_item
            })
            .collect();
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
}

/// Look up a single credit note by its hex ObjectId or unique model key (`cn_...`).
pub async fn get_credit_note(db: &Database, id_or_key: &str) -> AppResult<CreditNote> {
    let document = repository::credit_notes::find_credit_note_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Credit note not found", codes::CREDIT_NOTE_NOT_FOUND)
        })?;
    Ok(document.into_credit_note())
}

/// List credit notes matching query filters with pagination.
pub async fn list_credit_notes(
    db: &Database,
    query: CreditNoteListQuery,
) -> AppResult<CreditNoteListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        let mut or_clauses = vec![
            doc! { "credit_note_number": { "$regex": pattern.clone() } },
            doc! { "invoice_number": { "$regex": pattern.clone() } },
            doc! { "customer_name_snapshot": { "$regex": pattern } },
        ];

        let digits: String = search.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digits.parse::<u64>() {
            let padded_cn = format!("CN-{num:06}");
            let padded_inv = format!("INV-{num:06}");
            or_clauses.push(doc! { "credit_note_number": padded_cn });
            or_clauses.push(doc! { "invoice_number": padded_inv });
        }

        and_clauses.push(doc! { "$or": or_clauses });
    }
    if let Some(invoice_key) = query.invoice_key.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "invoice_key": invoice_key.trim() });
    }
    if let Some(customer_key) = query.customer_key.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "customer_key": customer_key.trim() });
    }
    if query.date_preset.as_deref() == Some("today") {
        let (today_start, today_end) = today_utc_range();
        and_clauses.push(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(today_start),
                "$lt": BsonDateTime::from_chrono(today_end),
            }
        });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let (documents, total) =
        repository::credit_notes::list_credit_notes(db, filter, skip, limit).await?;
    let credit_notes = documents
        .into_iter()
        .map(|d| d.into_credit_note())
        .collect();
    let total_pages = total.div_ceil(limit);

    Ok(CreditNoteListResponse {
        credit_notes,
        total,
        page,
        limit,
        total_pages,
    })
}

/// Converts a page of raw `credit_notes` documents — as read by the sync
/// module's cursor scan — into the `CreditNote` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<CreditNote>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<CreditNoteDocument>(document)?.into_credit_note())
        })
        .collect()
}
