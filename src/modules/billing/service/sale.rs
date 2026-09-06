// Sale completion, voiding, and closing — the centerpiece of the billing
// backend migration. `complete_sale` implements D4 from the migration
// plan: all validation/lookups happen before any writes, and once the
// `Invoice` (+ payment, if any) is durably inserted, no later sub-step
// failure (stock decrement, ticket status update, customer balance update)
// makes the request report as failed — each collects into `warnings`
// instead. This mirrors `purchases::service::purchase::record_purchase`'s
// accepted risk posture: no Mongo transactions exist anywhere in this
// codebase, so a partial failure here is the same class of exposure that
// module already has between "purchase inserted" and "stock bumped".

use std::collections::HashMap;

use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        calculations,
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        billing::{
            CompleteSaleResponse, CreateSaleItemRequest, CreateSaleRequest, Invoice, InvoiceItem,
            InvoiceStatus, PricingAdjustments, VoidInvoiceRequest,
        },
        inventory::{Product, StockMovementType},
        sequences::ReserveSequenceRequest,
    },
    modules::{
        billing::{model::InvoiceDocument, repository},
        customers, inventory, print_jobs, repairs, sequences,
    },
};

const VALID_PAYMENT_METHODS: &[&str] = &["cash", "card", "online", "split", "credit"];
const VALID_SOURCE_TYPES: &[&str] = &["retail", "repair", "print"];
const VALID_DISCOUNT_TYPES: &[&str] = &["percentage", "fixed"];

fn validate_sale_request(body: &CreateSaleRequest) -> AppResult<()> {
    if body.items.is_empty() {
        return Err(AppError::validation("A sale must have at least one item"));
    }
    for item in &body.items {
        if !VALID_SOURCE_TYPES.contains(&item.source_type.as_str()) {
            return Err(AppError::validation(format!(
                "Invalid item sourceType '{}'. Must be one of: {}",
                item.source_type,
                VALID_SOURCE_TYPES.join(", ")
            )));
        }
        if item.quantity < 1 {
            return Err(AppError::validation(format!(
                "An item of source type '{}' must have a quantity of at least 1",
                item.source_type
            )));
        }
        if item.discount_cents < 0 {
            return Err(AppError::validation(format!(
                "An item of source type '{}' cannot have a negative discount",
                item.source_type
            )));
        }
    }
    if !VALID_PAYMENT_METHODS.contains(&body.payment.payment_method.as_str()) {
        return Err(AppError::validation(format!(
            "Invalid paymentMethod '{}'. Must be one of: {}",
            body.payment.payment_method,
            VALID_PAYMENT_METHODS.join(", ")
        )));
    }
    if body.payment.payment_method == "split" {
        let legs = body.payment.split_payments.as_deref().unwrap_or(&[]);
        if legs.is_empty() {
            return Err(AppError::validation(
                "A split payment must include at least one split leg",
            ));
        }
    }
    if let Some(adjustments) = &body.pricing_adjustments {
        validate_pricing_adjustments(adjustments)?;
    }
    if body.staff.cashier_name.trim().is_empty() {
        return Err(AppError::validation("staff.cashierName is required"));
    }
    Ok(())
}

/// Resolves and validates the up-front partial payment on a credit sale.
/// Returns the amount (in cents) to record as a deposit payment now — `0`
/// for a plain "pay later" credit sale or any non-credit sale. A credit
/// deposit must be strictly less than the total (a deposit that covers the
/// whole invoice is a normal paid sale, not a credit sale) and, when
/// present, needs a real due date and a `"cash"`/`"card"` method.
fn resolve_credit_deposit(body: &CreateSaleRequest, total_cents: i64) -> AppResult<i64> {
    if !body.payment.is_credit {
        return Ok(0);
    }
    let deposit = body.payment.amount_received_cents.unwrap_or(0);
    if deposit <= 0 {
        return Ok(0);
    }
    if deposit >= total_cents {
        return Err(AppError::validation(
            "Amount paid now must be less than the invoice total for a credit sale",
        ));
    }
    match body.payment.deposit_method.as_deref() {
        None | Some("cash") | Some("card") => {}
        Some(other) => {
            return Err(AppError::validation(format!(
                "Invalid deposit method '{other}'. Must be 'cash' or 'card'"
            )));
        }
    }
    match body.payment.due_date.as_deref() {
        Some(due) if chrono::NaiveDate::parse_from_str(due, "%Y-%m-%d").is_ok() => {}
        _ => {
            return Err(AppError::validation(
                "A payment due date (YYYY-MM-DD) is required when the customer pays part of a credit sale now",
            ));
        }
    }
    Ok(deposit)
}

fn validate_pricing_adjustments(adjustments: &PricingAdjustments) -> AppResult<()> {
    match adjustments.discount_type.as_str() {
        "percentage" => {
            if !(0.0..=100.0).contains(&adjustments.discount_value) {
                return Err(AppError::validation(
                    "pricingAdjustments.discountValue must be between 0 and 100 for a percentage discount",
                ));
            }
        }
        "fixed" => {
            if adjustments.discount_value < 0.0 {
                return Err(AppError::validation(
                    "pricingAdjustments.discountValue cannot be negative for a fixed discount",
                ));
            }
        }
        other => {
            return Err(AppError::validation(format!(
                "Invalid pricingAdjustments.discountType '{other}'. Must be one of: {}",
                VALID_DISCOUNT_TYPES.join(", ")
            )));
        }
    }
    Ok(())
}

/// Resolves each request-side item into a persisted-shape `InvoiceItem`,
/// pulling `name`/`sku`/`unitPriceCents` (and, for repair/print, the
/// assigned employee) from the referenced product/ticket rather than
/// trusting the client — mirrors the customer-key resolution just above it
/// in `complete_sale`. Also returns the resolved `Product`s keyed by
/// `product_key` so the post-commit stock-decrement loop doesn't have to
/// look them up a second time. An item whose `productKey`/`sourceTicketKey`
/// doesn't resolve fails the whole request (via `?`) before any write,
/// exactly like a bad `customerKey` already does.
///
/// Retail products are fetched in one batched `$in` query up front (see
/// `inventory::service::product::get_products_by_keys`) rather than one
/// `get_product_by_key` round trip per item — a large cart previously paid 3
/// sequential Mongo round trips per retail line just for this lookup.
/// Remaining per-item work (repair/print ticket lookups) runs concurrently
/// via `try_join_all` instead of a sequential loop, so the whole resolution
/// step costs roughly one round trip's worth of latency regardless of cart
/// size, not N.
pub(crate) async fn resolve_sale_items(
    db: &Db,
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

    let resolved: Vec<InvoiceItem> = futures_util::future::try_join_all(
        items
            .iter()
            .map(|item| resolve_sale_item(db, item, &products)),
    )
    .await?;

    Ok((resolved, products))
}

pub(crate) async fn resolve_sale_item(
    db: &Db,
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

                if product.is_serialized {
                    let serial_numbers = item.serial_numbers.clone().unwrap_or_default();
                    if serial_numbers.len() as i64 != item.quantity {
                        return Err(AppError::validation(format!(
                            "'{}' is serialized — provide exactly {} serial number(s), got {}",
                            product.name,
                            item.quantity,
                            serial_numbers.len()
                        )));
                    }
                    // Fail before any write if a serial is unknown or
                    // already sold (D4's "resolve everything first" rule) —
                    // the actual `Sold` transition happens post-commit in
                    // `apply_line_item_side_effects`, same as the stock
                    // decrement it accompanies.
                    for serial_number in &serial_numbers {
                        inventory::service::product_serial::resolve_in_stock_serial(
                            db,
                            &product.key,
                            serial_number,
                        )
                        .await?;
                    }
                }

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

    let total_cents =
        calculations::compute_line_total(unit_price_cents, item.quantity, item.discount_cents);

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
        serial_numbers: item.serial_numbers.clone().unwrap_or_default(),
    })
}

/// One line item's post-commit side effect (stock decrement for retail,
/// ticket-delivered for repair/print) — factored out of `complete_sale` so
/// every item's side effect can run concurrently via `join_all` rather than
/// sequentially. Returns `Some(warning)` on a non-fatal failure, `None` on
/// success; never fails the request itself (D4).
async fn apply_line_item_side_effects(
    db: &Db,
    item: &InvoiceItem,
    resolved_products: &HashMap<String, Product>,
    inserted_invoice: &InvoiceDocument,
) -> Option<String> {
    match item.source_type.as_str() {
        "retail" => {
            let product_key = item.product_key.as_deref()?;
            // Already resolved (and guaranteed to exist) above — reuse it
            // rather than looking the product up again. It can only be
            // missing here if the item had no productKey, which the early
            // return above already excluded.
            let product = resolved_products.get(product_key)?;
            let object_id = match ObjectId::parse_str(&product.id) {
                Ok(id) => id,
                Err(_) => {
                    return Some(format!(
                        "Product '{}' has an invalid id and its stock was not adjusted",
                        item.name
                    ));
                }
            };
            if let Err(err) = inventory::service::stock::apply_stock_delta(
                db,
                object_id,
                -item.quantity,
                StockMovementType::Sale,
                Some(inserted_invoice.key.clone()),
                Some(format!(
                    "Sale on invoice {}",
                    inserted_invoice.invoice_number
                )),
            )
            .await
            {
                return Some(format!(
                    "Stock for '{}' could not be adjusted: {err}",
                    item.name
                ));
            }

            if product.is_serialized {
                for serial_number in &item.serial_numbers {
                    if let Ok(serial) = inventory::service::product_serial::resolve_in_stock_serial(
                        db,
                        &product.key,
                        serial_number,
                    )
                    .await
                        && let Some(serial_id) = serial.id
                    {
                        let _ = inventory::service::product_serial::mark_serial_sold(
                            db,
                            serial_id,
                            &inserted_invoice.key,
                            product.warranty_months,
                        )
                        .await;
                    }
                }
            }
            None
        }
        "repair" => match &item.source_ticket_key {
            Some(ticket_key) => {
                if let Err(err) = repairs::service::mark_delivered(db, ticket_key).await {
                    Some(format!(
                        "Repair ticket {} could not be marked delivered: {err}",
                        item.source_ticket_number.as_deref().unwrap_or(ticket_key)
                    ))
                } else {
                    None
                }
            }
            None => Some(format!(
                "Item '{}' is a repair line with no ticket key; its ticket was not updated",
                item.name
            )),
        },
        "print" => match &item.source_ticket_key {
            Some(ticket_key) => {
                if let Err(err) = print_jobs::service::mark_delivered(db, ticket_key).await {
                    Some(format!(
                        "Print job ticket {} could not be marked delivered: {err}",
                        item.source_ticket_number.as_deref().unwrap_or(ticket_key)
                    ))
                } else {
                    None
                }
            }
            None => Some(format!(
                "Item '{}' is a print-job line with no ticket key; its ticket was not updated",
                item.name
            )),
        },
        _ => None,
    }
}

pub async fn complete_sale(
    db: &Db,
    body: CreateSaleRequest,
    cashier_id: String,
    device_id: Option<String>,
) -> AppResult<CompleteSaleResponse> {
    validate_sale_request(&body)?;

    // Resolve the linked customer, if any, before any write — a bad
    // customerKey should fail the whole request, not half-complete it.
    let customer_key = body.customer.as_ref().and_then(|c| c.customer_key.clone());
    let resolved_customer = match &customer_key {
        Some(key) => Some(customers::service::get_customer_by_key(db, key).await?),
        None => None,
    };

    let (customer_name_snapshot, customer_phone_snapshot, customer_address_snapshot) =
        match &resolved_customer {
            Some(customer) => (
                Some(customer.name.clone()),
                Some(customer.primary_phone.clone()),
                customer.address.clone(),
            ),
            None => {
                let customer_ref = body.customer.as_ref();
                (
                    customer_ref.and_then(|c| c.customer_name.clone()),
                    customer_ref.and_then(|c| c.customer_phone.clone()),
                    customer_ref.and_then(|c| c.customer_address.clone()),
                )
            }
        };

    // Same "fail before any write" treatment as the customer lookup above —
    // an unresolvable productKey/sourceTicketKey can't produce a sensible
    // line item, so the whole sale is rejected rather than half-completed.
    let (resolved_items, resolved_products) = resolve_sale_items(db, &body.items).await?;

    // subtotal/discount/total/change are calculated via the authoritative calculation engine.
    let (subtotal_cents, discount_type, discount_value, discount_cents, total_cents) =
        calculations::compute_sale_totals(&resolved_items, body.pricing_adjustments.as_ref());
    let change_due_cents =
        calculations::compute_change_due(body.payment.amount_received_cents, total_cents);

    // An up-front partial payment on a credit sale ("pay Rs 500 now, rest on
    // account"). `0` for a plain credit sale or any non-credit sale.
    let credit_deposit_cents = resolve_credit_deposit(&body, total_cents)?;

    if body.payment.payment_method == "split" {
        let legs = body.payment.split_payments.as_deref().unwrap_or(&[]);
        calculations::validate_split_payments(total_cents, legs)?;
    }

    let reservation = sequences::service::reserve_sequence(
        db,
        "invoice".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id: device_id.clone(),
        },
    )
    .await?;
    let invoice_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );

    let status = if body.payment.is_credit {
        InvoiceStatus::from_payment_progress(total_cents, credit_deposit_cents)
    } else {
        InvoiceStatus::Paid
    };
    // Method the deposit leg is recorded under (invoice.payment_method stays
    // "credit"); also mirrored onto the invoice's card fields when it was a
    // card deposit so downstream readers (dashboard cash-drawer split) can
    // tell how the up-front money came in.
    let deposit_method = body.payment.deposit_method.as_deref().unwrap_or("cash");
    let deposit_is_card = credit_deposit_cents > 0 && deposit_method == "card";
    let now = BsonDateTime::now();

    let invoice_document = InvoiceDocument {
        id: None,
        key: generate_id(prefixes::INVOICE),
        invoice_number,
        customer_key: customer_key.clone(),
        customer_name_snapshot,
        customer_phone_snapshot,
        customer_address_snapshot,
        cashier_id: cashier_id.clone(),
        cashier_name_snapshot: body.staff.cashier_name.clone(),
        items: resolved_items.clone(),
        subtotal_cents,
        discount_type,
        discount_value,
        discount_cents,
        total_cents,
        payment_method: body.payment.payment_method.clone(),
        split_payments: body.payment.split_payments.clone(),
        is_credit: body.payment.is_credit,
        amount_received_cents: body.payment.amount_received_cents,
        change_due_cents,
        due_date: body.payment.due_date.clone(),
        card_last4: if body.payment.is_credit && !deposit_is_card {
            None
        } else {
            body.payment.card_last4.clone()
        },
        card_ref: body.payment.card_ref.clone(),
        online_ref: body.payment.online_ref.clone(),
        online_note: body.payment.online_note.clone(),
        status,
        notes: body.notes.clone(),
        shop_profile_snapshot: body.shop_profile_snapshot.clone(),
        warranty_terms_snapshot: body.warranty_terms_snapshot.clone(),
        document_selection: body.document_selection.clone(),
        voided_at: None,
        voided_by: None,
        voided_reason: None,
        closed_at: None,
        closed_by: None,
        refunded_cents: 0,
        credit_note_count: 0,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    // From this point on, the sale is committed: nothing below reports
    // failure back to the cashier (D4) — every sub-step collects into
    // `warnings` instead.
    let inserted_invoice = repository::insert_invoice(db, invoice_document).await?;
    let mut warnings: Vec<String> = Vec::new();

    let mut payment_documents = Vec::new();
    if !body.payment.is_credit {
        // Matches the mock's simplicity (`mockInvoices.ts::createInvoice`):
        // a non-credit sale is fully paid at completion, regardless of the
        // tendered/change breakdown recorded for display.
        let legs: Vec<(String, i64, Option<String>)> = match &body.payment.split_payments {
            Some(split) if body.payment.payment_method == "split" => split
                .iter()
                .map(|leg| (leg.method.clone(), leg.amount_cents, leg.card_last4.clone()))
                .collect(),
            _ => vec![(
                body.payment.payment_method.clone(),
                total_cents,
                body.payment.card_last4.clone(),
            )],
        };

        for (method, amount_cents, card_last4) in legs {
            let payment_document = crate::modules::billing::model::PaymentDocument {
                id: None,
                key: generate_id(prefixes::PAYMENT),
                invoice_key: inserted_invoice.key.clone(),
                amount_cents,
                payment_method: method,
                notes: card_last4.map(|last4| format!("Card ending {last4}")),
                recorded_by_user_id: cashier_id.clone(),
                recorded_by_name_snapshot: body.staff.cashier_name.clone(),
                recorded_at: now,
                version: 1,
                created_at: now,
                updated_at: now,
            };
            match repository::insert_payment(db, payment_document).await {
                Ok(inserted) => payment_documents.push(inserted),
                Err(err) => warnings.push(format!(
                    "Invoice {} was created but a payment record failed to save: {err}",
                    inserted_invoice.invoice_number
                )),
            }
        }
    } else if credit_deposit_cents > 0 {
        // Up-front deposit on a credit sale. Recorded as a real payment row
        // so the invoice reads `partially_paid` and the outstanding balance
        // is derived (total − Σ payments) everywhere, exactly like a later
        // installment. The customer-balance effect of this deposit is folded
        // into the single `apply_financial_delta` call below — it must NOT
        // also go through `service::payments`, or the balance would be
        // decremented twice.
        let deposit_document = crate::modules::billing::model::PaymentDocument {
            id: None,
            key: generate_id(prefixes::PAYMENT),
            invoice_key: inserted_invoice.key.clone(),
            amount_cents: credit_deposit_cents,
            payment_method: deposit_method.to_string(),
            notes: Some(
                match (deposit_is_card, body.payment.card_last4.as_deref()) {
                    (true, Some(last4)) if !last4.is_empty() => {
                        format!("Deposit at checkout — card ending {last4}")
                    }
                    _ => "Deposit at checkout".to_string(),
                },
            ),
            recorded_by_user_id: cashier_id.clone(),
            recorded_by_name_snapshot: body.staff.cashier_name.clone(),
            recorded_at: now,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        match repository::insert_payment(db, deposit_document).await {
            Ok(inserted) => payment_documents.push(inserted),
            Err(err) => warnings.push(format!(
                "Invoice {} was created but the deposit payment record failed to save: {err}",
                inserted_invoice.invoice_number
            )),
        }
    }

    // Run every line item's stock/ticket side effect concurrently instead of
    // one-at-a-time — each retail line pays up to 3 sequential Mongo round
    // trips inside `apply_stock_delta`, so a large cart processed serially
    // here was the other half (alongside item resolution above) of what
    // pushed checkout past the frontend's request timeout. Order relative to
    // `warnings` no longer matters (D4: this section only ever collects
    // warnings, never fails the request), so collecting results after the
    // fact is equivalent to the old sequential push.
    let side_effect_warnings: Vec<Option<String>> =
        futures_util::future::join_all(resolved_items.iter().map(|item| {
            apply_line_item_side_effects(db, item, &resolved_products, &inserted_invoice)
        }))
        .await;
    warnings.extend(side_effect_warnings.into_iter().flatten());

    if let Some(customer_key) = &customer_key {
        // Total spend always grows by the full invoice; the owed balance
        // grows only by the part left on account (total minus any deposit
        // taken now).
        let balance_delta = if body.payment.is_credit {
            total_cents - credit_deposit_cents
        } else {
            0
        };
        if let Err(err) =
            customers::service::apply_financial_delta(db, customer_key, total_cents, balance_delta)
                .await
        {
            warnings.push(format!("Customer financials could not be updated: {err}"));
        }
    }

    Ok(CompleteSaleResponse {
        invoice: inserted_invoice.into_invoice(),
        payments: payment_documents
            .into_iter()
            .map(|doc| doc.into_payment_record())
            .collect(),
        warnings,
    })
}

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
        match inventory::service::product::get_product_by_key(db, product_key).await {
            Ok(product) => {
                let Ok(product_object_id) = ObjectId::parse_str(&product.id) else {
                    warnings.push(format!(
                        "Product '{}' has an invalid id and its stock was not restored",
                        item.name
                    ));
                    continue;
                };
                if let Err(err) = inventory::service::stock::apply_stock_delta(
                    db,
                    product_object_id,
                    item.quantity,
                    StockMovementType::InvoiceVoidReversal,
                    Some(existing.key.clone()),
                    Some(format!("Void of invoice {}", existing.invoice_number)),
                )
                .await
                {
                    warnings.push(format!(
                        "Stock for '{}' could not be restored: {err}",
                        item.name
                    ));
                }
            }
            Err(err) => warnings.push(format!(
                "Product '{}' (key {product_key}) was not found; its stock was not restored: {err}",
                item.name
            )),
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
}

/// Marks a fully paid invoice `Closed` — a manual, explicit "done, nothing
/// more will happen to this invoice" action (never auto-computed). Blocked
/// unless the invoice is `Paid` and has no open (non-voided) credit notes
/// against it; re-queries `credit_notes` live for that guard rather than
/// trusting `Invoice.credit_note_count` alone.
pub async fn close_invoice(db: &Db, id_or_key: &str, closed_by: String) -> AppResult<Invoice> {
    let existing = repository::find_invoice_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    if existing.status != InvoiceStatus::Paid {
        return Err(AppError::conflict(
            codes::INVOICE_NOT_CLOSABLE,
            "Only a fully paid invoice can be closed",
        ));
    }

    let open_credit_notes =
        repository::credit_notes::count_open_credit_notes_for_invoice(db, &existing.key).await?;
    if open_credit_notes > 0 {
        return Err(AppError::conflict(
            codes::INVOICE_NOT_CLOSABLE,
            "This invoice has an open credit note — void or resolve it before closing",
        ));
    }

    let object_id = existing
        .id
        .expect("persisted invoice document must have an _id");
    let set_doc = doc! {
        "status": InvoiceStatus::Closed.as_str(),
        "closed_at": BsonDateTime::now(),
        "closed_by": &closed_by,
        "updated_at": BsonDateTime::now(),
    };

    let updated = repository::update_invoice_status(db, object_id, set_doc)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    Ok(updated.into_invoice())
}
