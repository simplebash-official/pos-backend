use super::service_sale::resolve_sale_items;
use crate::clients::db::Db;
use crate::core::calculations;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::billing::{
    CompleteSaleResponse, CreateSaleRequest, InvoiceItem, InvoiceStatus, PricingAdjustments,
};
use crate::domain::inventory::{Product, StockMovementType};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::warranties::ProductWarranty;
use crate::modules::billing::model::InvoiceDocument;
use crate::modules::billing::repository;
use crate::modules::{customers, inventory, print_jobs, repairs, sequences, warranties};
use mongodb::bson::DateTime as BsonDateTime;
use std::collections::HashMap;

const VALID_PAYMENT_METHODS: &[&str] = &["cash", "card", "online", "split", "credit"];

const VALID_SOURCE_TYPES: &[&str] = &["retail", "repair", "print"];

const VALID_DISCOUNT_TYPES: &[&str] = &["percentage", "fixed"];

fn validate_sale_request(body: &CreateSaleRequest) -> AppResult<()> {
    if body.items.is_empty() {
        return Err(AppError::validation("A sale must have at least one item"));
    }
    for item in &body.items {
        if item.warranty_override.is_some()
            && (item.source_type != "retail" || item.product_key.is_none())
        {
            return Err(AppError::validation(
                "Only a catalogue item can have its warranty changed at the till.",
            ));
        }
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

/// What every line's post-commit step needs to know about the sale.
#[derive(Debug)]
struct SaleContext<'a> {
    invoice: &'a InvoiceDocument,
    /// Per request line, in order: the cashier's warranty for it, when it was changed.
    warranty_overrides: &'a [Option<ProductWarranty>],
    actor: &'a warranties::service::WarrantyActor,
    device_id: Option<String>,
}

/// One line item's post-commit side effect (stock decrement for retail,
/// ticket-delivered for repair/print) — factored out of `complete_sale` so
/// every item's side effect can run concurrently via `join_all` rather than
/// sequentially. Returns `Some(warning)` on a non-fatal failure, `None` on
/// success; never fails the request itself (D4).
async fn apply_line_item_side_effects(
    db: &Db,
    line_index: usize,
    item: &InvoiceItem,
    resolved_products: &HashMap<String, Product>,
    sale: &SaleContext<'_>,
) -> Option<String> {
    let inserted_invoice = sale.invoice;
    match item.source_type.as_str() {
        "retail" => {
            let product_key = item.product_key.as_deref()?;
            // Already resolved (and guaranteed to exist) above — reuse it
            // rather than looking the product up again. It can only be
            // missing here if the item had no productKey, which the early
            // return above already excluded.
            let product = resolved_products.get(product_key)?;
            if let Err(err) = inventory::service::stock_batch::move_allocations(
                db,
                &product.key,
                &item.batch_allocations,
                -1,
                StockMovementType::Sale,
                &inserted_invoice.key,
                &format!("Sale on invoice {}", inserted_invoice.invoice_number),
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
                    let serial = match inventory::service::product_serial::resolve_in_stock_serial(
                        db,
                        &product.key,
                        serial_number,
                    )
                    .await
                    {
                        Ok(serial) => serial,
                        Err(err) => {
                            return Some(format!(
                                "Serial {serial_number} of '{}' could not be marked sold: {err}",
                                item.name
                            ));
                        }
                    };
                    let Some(serial_id) = serial.id else {
                        return Some(format!(
                            "Serial {serial_number} of '{}' has no stored id and was not marked sold",
                            item.name
                        ));
                    };
                    if let Err(err) = inventory::service::product_serial::mark_serial_sold(
                        db,
                        serial_id,
                        &inserted_invoice.key,
                    )
                    .await
                    {
                        return Some(format!(
                            "Serial {serial_number} of '{}' could not be marked sold: {err}",
                            item.name
                        ));
                    }
                }
            }

            let warranty = match &sale.warranty_overrides[line_index] {
                Some(chosen) => chosen,
                None => &product.warranty,
            };
            if let Err(err) = warranties::service::issue_for_sale_line(
                db,
                warranties::service::SaleLine {
                    invoice_key: &inserted_invoice.key,
                    invoice_number: &inserted_invoice.invoice_number,
                    sold_at: inserted_invoice.created_at.to_chrono(),
                    customer_key: inserted_invoice.customer_key.as_deref(),
                    customer_name: inserted_invoice.customer_name_snapshot.as_deref(),
                    customer_phone: inserted_invoice.customer_phone_snapshot.as_deref(),
                    line_index,
                    product_key: &product.key,
                    item_name: &item.name,
                    quantity: item.quantity,
                    unit_price_cents: item.unit_price_cents,
                    serial_numbers: &item.serial_numbers,
                    warranty,
                    actor: sale.actor,
                    device_id: sale.device_id.clone(),
                },
            )
            .await
            {
                return Some(format!(
                    "The warranty for '{}' could not be recorded: {err}",
                    item.name
                ));
            }
            None
        }
        "repair" => match &item.source_ticket_key {
            Some(ticket_key) => {
                let ticket_label = item.source_ticket_number.as_deref().unwrap_or(ticket_key);
                let repair = match repairs::service::mark_delivered(db, ticket_key).await {
                    Ok(repair) => repair,
                    Err(err) => {
                        return Some(format!(
                            "Repair ticket {ticket_label} could not be marked delivered: {err}"
                        ));
                    }
                };
                if let Err(err) = repairs::service::use_parts(
                    db,
                    &repair,
                    StockMovementType::RepairPartConsumption,
                    &inserted_invoice.key,
                )
                .await
                {
                    return Some(format!(
                        "The parts of repair ticket {ticket_label} could not be taken out of stock: {err}"
                    ));
                }
                if let Err(err) = warranties::service::issue_for_repair(
                    db,
                    warranties::service::RepairLine {
                        invoice_key: &inserted_invoice.key,
                        invoice_number: &inserted_invoice.invoice_number,
                        delivered_at: inserted_invoice.created_at.to_chrono(),
                        customer_key: inserted_invoice.customer_key.as_deref(),
                        customer_name: inserted_invoice.customer_name_snapshot.as_deref(),
                        customer_phone: inserted_invoice.customer_phone_snapshot.as_deref(),
                        line_index,
                        repair_key: &repair.key,
                        ticket_number: &repair.ticket_number,
                        device_model: &repair.device_model,
                        device_serial: repair.serial_number.as_deref(),
                        items: &repair.items,
                        actor: sale.actor,
                        device_id: sale.device_id.clone(),
                    },
                )
                .await
                {
                    return Some(format!(
                        "The warranties of repair ticket {ticket_label} could not be recorded: {err}"
                    ));
                }
                None
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

/// Completes the sale.
pub async fn complete_sale(
    db: &Db,
    body: CreateSaleRequest,
    cashier_id: String,
    device_id: Option<String>,
) -> AppResult<CompleteSaleResponse> {
    crate::core::logging::domain::tracked("billing.sale_completed", async move {
        validate_sale_request(&body)?;
        let warranty_overrides = body
            .items
            .iter()
            .map(|item| {
                item.warranty_override
                    .clone()
                    .map(ProductWarranty::validated)
                    .transpose()
            })
            .collect::<AppResult<Vec<_>>>()?;

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

        // Resolve and lock-check the register session (rejects closed or mid-count closing shifts).
        let session_doc = crate::modules::sessions::service::resolve_writable_session(
            db,
            body.session_key.as_deref(),
            &cashier_id,
        )
        .await?;
        let active_session_key = session_doc.as_ref().map(|s| s.key.clone());
        // Cash updates the shop's shared drawer in SharedShop mode, or the shift owner's drawer in Personal mode.
        let active_session_owner = match session_doc.as_ref() {
            Some(sess) => Some(
                crate::modules::sessions::service::active_drawer_key(db, Some(sess), &cashier_id)
                    .await?,
            ),
            None => None,
        };

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
            session_key: active_session_key.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: device_id.clone(),
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
                    session_key: active_session_key.clone(),
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
                session_key: active_session_key.clone(),
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

        // Track payment breakdown against register session and cash drawer
        let (cash_delta, card_delta, online_delta, credit_delta) = if !body.payment.is_credit {
            let mut cash = 0i64;
            let mut card = 0i64;
            let mut online = 0i64;
            match &body.payment.split_payments {
                Some(split) if body.payment.payment_method == "split" => {
                    for leg in split {
                        match leg.method.as_str() {
                            "cash" => cash += leg.amount_cents,
                            "card" => card += leg.amount_cents,
                            "online" => online += leg.amount_cents,
                            _ => {}
                        }
                    }
                }
                _ => match body.payment.payment_method.as_str() {
                    "cash" => cash += total_cents,
                    "card" => card += total_cents,
                    "online" => online += total_cents,
                    _ => {}
                },
            }
            (cash, card, online, 0i64)
        } else {
            let unpaid = total_cents - credit_deposit_cents;
            let (cash, card) = if credit_deposit_cents > 0 {
                if deposit_method == "card" {
                    (0i64, credit_deposit_cents)
                } else {
                    (credit_deposit_cents, 0i64)
                }
            } else {
                (0i64, 0i64)
            };
            (cash, card, 0i64, unpaid)
        };

        if let (Some(sess_key), Some(owner)) = (&active_session_key, &active_session_owner) {
            if let Err(err) =
                crate::modules::sessions::repository::sessions::increment_session_sales(
                    db,
                    sess_key,
                    cash_delta,
                    card_delta,
                    online_delta,
                    credit_delta,
                )
                .await
            {
                warnings.push(format!(
                    "Register session sales counters failed to update: {err}"
                ));
            }

            if cash_delta != 0
                && let Err(err) =
                    crate::modules::sessions::repository::drawer::update_drawer_balance(
                        db, owner, cash_delta,
                    )
                    .await
            {
                warnings.push(format!("Cash drawer balance failed to update: {err}"));
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
        let actor = warranties::service::WarrantyActor {
            user_id: cashier_id.clone(),
            name: body.staff.cashier_name.trim().to_string(),
        };
        let sale = SaleContext {
            invoice: &inserted_invoice,
            warranty_overrides: &warranty_overrides,
            actor: &actor,
            device_id: device_id.clone(),
        };
        let side_effect_warnings: Vec<Option<String>> = match db {
            Db::Mongo(_) => {
                futures_util::future::join_all(resolved_items.iter().enumerate().map(
                    |(line_index, item)| {
                        apply_line_item_side_effects(
                            db,
                            line_index,
                            item,
                            &resolved_products,
                            &sale,
                        )
                    },
                ))
                .await
            }
            Db::Sqlite(_) => {
                let mut results = Vec::with_capacity(resolved_items.len());
                for (line_index, item) in resolved_items.iter().enumerate() {
                    results.push(
                        apply_line_item_side_effects(
                            db,
                            line_index,
                            item,
                            &resolved_products,
                            &sale,
                        )
                        .await,
                    );
                }
                results
            }
        };
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
            if let Err(err) = customers::service::apply_financial_delta(
                db,
                customer_key,
                total_cents,
                balance_delta,
            )
            .await
            {
                warnings.push(format!("Customer financials could not be updated: {err}"));
            }
        }

        if let Some(quotation_key) = &body.quotation_key
            && let Err(err) = crate::modules::quotations::service::fulfill_quotation(
                db,
                quotation_key,
                &inserted_invoice.key,
                device_id.clone(),
            )
            .await
        {
            warnings.push(format!(
                "Quotation '{quotation_key}' could not be marked as fulfilled: {err}"
            ));
        }

        Ok(CompleteSaleResponse {
            invoice: inserted_invoice.into_invoice(),
            payments: payment_documents
                .into_iter()
                .map(|doc| doc.into_payment_record())
                .collect(),
            warnings,
        })
    })
    .await
}
