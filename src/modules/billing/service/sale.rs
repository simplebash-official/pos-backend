// Sale completion and cancellation — the centerpiece of the billing
// backend migration. `complete_sale` implements D4 from the migration
// plan: all validation/lookups happen before any writes, and once the
// `Invoice` (+ payment, if any) is durably inserted, no later sub-step
// failure (stock decrement, ticket status update, customer balance update)
// makes the request report as failed — each collects into `warnings`
// instead. This mirrors `purchases::service::purchase::record_purchase`'s
// accepted risk posture: no Mongo transactions exist anywhere in this
// codebase, so a partial failure here is the same class of exposure that
// module already has between "purchase inserted" and "stock bumped".

use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, doc, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        billing::{CancelInvoiceRequest, CompleteSaleResponse, CreateSaleRequest, Invoice},
        inventory::StockMovementType,
        sequences::ReserveSequenceRequest,
    },
    modules::{
        billing::{model::InvoiceDocument, repository},
        customers, inventory, print_jobs, repairs, sequences,
    },
};

const VALID_PAYMENT_METHODS: &[&str] = &["cash", "card", "online", "split", "credit"];
const VALID_SOURCE_TYPES: &[&str] = &["retail", "repair", "print"];

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
                "Item '{}' must have a quantity of at least 1",
                item.name
            )));
        }
        if item.total_cents < 0 {
            return Err(AppError::validation(format!(
                "Item '{}' cannot have a negative total",
                item.name
            )));
        }
    }
    if !VALID_PAYMENT_METHODS.contains(&body.payment_method.as_str()) {
        return Err(AppError::validation(format!(
            "Invalid paymentMethod '{}'. Must be one of: {}",
            body.payment_method,
            VALID_PAYMENT_METHODS.join(", ")
        )));
    }
    if body.payment_method == "split" {
        let legs = body.split_payments.as_deref().unwrap_or(&[]);
        if legs.is_empty() {
            return Err(AppError::validation(
                "A split payment must include at least one split leg",
            ));
        }
        let legs_total: i64 = legs.iter().map(|leg| leg.amount_cents).sum();
        if legs_total > body.total_cents {
            return Err(AppError::validation(
                "Split payment legs cannot sum to more than the invoice total",
            ));
        }
    }
    let expected_total = (body.subtotal_cents - body.discount_cents + body.tax_cents).max(0);
    if expected_total != body.total_cents {
        return Err(AppError::validation(format!(
            "totalCents ({}) does not match subtotalCents - discountCents + taxCents ({})",
            body.total_cents, expected_total
        )));
    }
    if body.cashier_name.trim().is_empty() {
        return Err(AppError::validation("cashierName is required"));
    }
    Ok(())
}

pub async fn complete_sale(
    db: &Database,
    body: CreateSaleRequest,
    cashier_id: String,
    device_id: Option<String>,
) -> AppResult<CompleteSaleResponse> {
    validate_sale_request(&body)?;

    // Resolve the linked customer, if any, before any write — a bad
    // customerKey should fail the whole request, not half-complete it.
    let resolved_customer = if let Some(ref key) = body.customer_key {
        Some(customers::service::get_customer_by_key(db, key).await?)
    } else {
        None
    };

    let (customer_name_snapshot, customer_phone_snapshot, customer_address_snapshot) =
        match &resolved_customer {
            Some(customer) => (
                Some(customer.name.clone()),
                Some(customer.primary_phone.clone()),
                customer.address.clone(),
            ),
            None => (
                body.customer_name.clone(),
                body.customer_phone.clone(),
                body.customer_address.clone(),
            ),
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

    let status = if body.is_credit { "pending" } else { "paid" };
    let now = BsonDateTime::now();

    let invoice_document = InvoiceDocument {
        id: None,
        key: generate_id(prefixes::INVOICE),
        invoice_number,
        customer_key: body.customer_key.clone(),
        customer_name_snapshot,
        customer_phone_snapshot,
        customer_address_snapshot,
        cashier_id: cashier_id.clone(),
        cashier_name_snapshot: body.cashier_name.clone(),
        items: body.items.clone(),
        subtotal_cents: body.subtotal_cents,
        discount_cents: body.discount_cents,
        tax_cents: body.tax_cents,
        total_cents: body.total_cents,
        payment_method: body.payment_method.clone(),
        split_payments: body.split_payments.clone(),
        is_credit: body.is_credit,
        tendered_amount_cents: body.tendered_amount_cents,
        change_due_cents: body.change_due_cents,
        due_date: body.due_date.clone(),
        card_last4: body.card_last4.clone(),
        card_ref: body.card_ref.clone(),
        online_ref: body.online_ref.clone(),
        online_note: body.online_note.clone(),
        status: status.to_string(),
        notes: body.notes.clone(),
        shop_profile_snapshot: body.shop_profile_snapshot.clone(),
        warranty_terms_snapshot: body.warranty_terms_snapshot.clone(),
        document_selection: body.document_selection.clone(),
        cancelled_at: None,
        cancelled_by: None,
        cancellation_reason: None,
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
    if !body.is_credit {
        // Matches the mock's simplicity (`mockInvoices.ts::createInvoice`):
        // a non-credit sale is fully paid at completion, regardless of the
        // tendered/change breakdown recorded for display.
        let legs: Vec<(String, i64, Option<String>)> = match &body.split_payments {
            Some(split) if body.payment_method == "split" => split
                .iter()
                .map(|leg| (leg.method.clone(), leg.amount_cents, leg.card_last4.clone()))
                .collect(),
            _ => vec![(
                body.payment_method.clone(),
                body.total_cents,
                body.card_last4.clone(),
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
                recorded_by_name_snapshot: body.cashier_name.clone(),
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
    }

    for item in &body.items {
        match item.source_type.as_str() {
            "retail" => {
                let Some(product_key) = item.product_key.as_deref() else {
                    continue;
                };
                match inventory::service::product::get_product_by_key(db, product_key).await {
                    Ok(product) => {
                        let Ok(object_id) = ObjectId::parse_str(&product.id) else {
                            warnings.push(format!(
                                "Product '{}' has an invalid id and its stock was not adjusted",
                                item.name
                            ));
                            continue;
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
                            warnings.push(format!(
                                "Stock for '{}' could not be adjusted: {err}",
                                item.name
                            ));
                        }
                    }
                    Err(err) => warnings.push(format!(
                        "Product '{}' (key {product_key}) was not found; its stock was not adjusted: {err}",
                        item.name
                    )),
                }
            }
            "repair" => match &item.source_ticket_key {
                Some(ticket_key) => {
                    if let Err(err) = repairs::service::mark_delivered(db, ticket_key).await {
                        warnings.push(format!(
                            "Repair ticket {} could not be marked delivered: {err}",
                            item.source_ticket_number.as_deref().unwrap_or(ticket_key)
                        ));
                    }
                }
                None => warnings.push(format!(
                    "Item '{}' is a repair line with no ticket key; its ticket was not updated",
                    item.name
                )),
            },
            "print" => match &item.source_ticket_key {
                Some(ticket_key) => {
                    if let Err(err) = print_jobs::service::mark_delivered(db, ticket_key).await {
                        warnings.push(format!(
                            "Print job ticket {} could not be marked delivered: {err}",
                            item.source_ticket_number.as_deref().unwrap_or(ticket_key)
                        ));
                    }
                }
                None => warnings.push(format!(
                    "Item '{}' is a print-job line with no ticket key; its ticket was not updated",
                    item.name
                )),
            },
            _ => {}
        }
    }

    if let Some(customer_key) = &body.customer_key {
        let balance_delta = if body.is_credit { body.total_cents } else { 0 };
        if let Err(err) = customers::service::apply_financial_delta(
            db,
            customer_key,
            body.total_cents,
            balance_delta,
        )
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

/// Basic cancel/void (D8): reverses retail stock and customer financials,
/// sets `status: "cancelled"`. Does **not** touch repair/print-job ticket
/// status (ambiguous what "un-delivering" a ticket should mean — flagged
/// as a warning for manual follow-up instead) and refuses to run at all if
/// any payment beyond the original sale-time one has been recorded
/// (reversing partial credit repayments is out of scope for this basic
/// version).
pub async fn cancel_invoice(
    db: &Database,
    id_or_key: &str,
    body: CancelInvoiceRequest,
    cancelled_by: String,
) -> AppResult<(Invoice, Vec<String>)> {
    let existing = repository::find_invoice_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    if existing.status == "cancelled" {
        return Err(AppError::conflict(
            codes::INVOICE_ALREADY_CANCELLED,
            "This invoice has already been cancelled",
        ));
    }

    // How many payment rows `complete_sale` itself would have inserted —
    // zero for a credit sale, one per split leg for a split payment,
    // otherwise exactly one. Any payment beyond that count means a
    // repayment has since been recorded, which this basic cancel flow
    // cannot safely reverse (see the fn doc comment).
    let expected_payment_count: usize = if existing.is_credit {
        0
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
            codes::INVOICE_HAS_PAYMENTS_CANNOT_CANCEL,
            "This invoice has payments recorded beyond the original sale and cannot be cancelled automatically",
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
                    "Invoice cancelled — ticket for '{}' was not reverted and may need manual review",
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
                    StockMovementType::Return,
                    Some(existing.key.clone()),
                    Some(format!(
                        "Cancellation of invoice {}",
                        existing.invoice_number
                    )),
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
        let balance_delta = if existing.is_credit {
            -existing.total_cents
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

    let mut set_doc = doc! {
        "status": "cancelled",
        "cancelled_at": BsonDateTime::now(),
        "cancelled_by": &cancelled_by,
        "updated_at": BsonDateTime::now(),
    };
    if let Some(reason) = body.reason {
        set_doc.insert("cancellation_reason", reason);
    }

    let updated = repository::update_invoice_status(db, object_id, set_doc)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    Ok((updated.into_invoice(), warnings))
}
