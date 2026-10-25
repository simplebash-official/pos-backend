// Business rules for supplier receipts: one invoice's worth of stock received at once. A receipt
// is validated and its supplier and products resolved before anything is written; then the
// header, every line, every line's stock batch (with its `PurchaseReceipt` movement) and every
// serial are written in one transaction, so a failure leaves no half-received invoice behind.
// The units come in through `inventory::service::stock_batch::create_batch`, the one place
// stock is ever created.

use std::collections::{HashMap, HashSet};

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        cash_categories::category_key,
        expenses::{CreateExpenseRequest, PaidFrom},
        inventory::{BatchSource, PaginationMeta, StockMovementType},
        purchase_receipts::{
            CreateReceiptRequest, PaymentKind, PaymentSource, PurchaseReceipt, ReceiptListQuery,
            ReceiptListResponse, ReceiptPayment, ReceiptSettlement,
        },
        purchases::SupplierSummary,
        sequences::ReserveSequenceRequest,
        sessions::{CashMovementType, CreateCashMovementRequest},
    },
    modules::{
        expenses::service::{self as expense_service, PreparedExpense},
        inventory::service::{
            product as inventory_product, product_serial as inventory_product_serial,
            stock_batch as inventory_stock_batch,
        },
        purchases::{
            model::{
                PurchaseDocument, PurchaseReceiptDocument, ReceiptFigures,
                ReceiptSettlementDocument,
            },
            repository,
            service::{credit as credit_service, purchase::enrich_purchases},
        },
        sequences,
        sessions::{model::CashMovementDocument, service::movement as movement_service},
        suppliers::service as supplier_service,
    },
};

/// Upper bound when every receipt of a supplier has to be looked at.
const ALL_RECEIPTS_LIMIT: i64 = 10_000;
const REFERENCE_MAX: usize = 60;
const NOTES_MAX: usize = 200;

/// Who is recording the receipt, for the payment made with it.
pub struct RecordedBy {
    pub user_id: String,
    pub name: String,
}

/// A payment validated and ready to write, whichever account it comes out of.
enum PreparedPayment {
    /// A drawer `supplier_payment`, with the owner's capital put in first when the owner paid.
    Cash(Box<CashPayment>),
    /// A payment on a ledger expense, from the bank, a card or online.
    Ledger(Box<PreparedExpense>),
}

struct CashPayment {
    capital: Option<CashMovementDocument>,
    payment: CashMovementDocument,
}

/// Checks the payment made with a receipt and prepares the records it writes. Nothing is
/// written; the receipt number is already known, so the payment can say what it was for.
#[allow(clippy::too_many_arguments)]
async fn prepare_payment(
    db: &Db,
    by: RecordedBy,
    supplier_key: &str,
    supplier_name: &str,
    receipt_key: &str,
    receipt_number: &str,
    reference_no: Option<&str>,
    total_cents: i64,
    payment: ReceiptPayment,
    device_id: Option<String>,
) -> AppResult<PreparedPayment> {
    if payment.amount_cents <= 0 {
        return Err(AppError::validation(
            "The amount paid must be more than zero",
        ));
    }
    if payment.amount_cents > total_cents {
        return Err(AppError::conflict(
            codes::RECEIPT_OVERPAID,
            "More is being paid than the receipt comes to.",
        ));
    }
    let note = format!("Receipt {receipt_number}");

    match payment.source {
        PaymentSource::Drawer | PaymentSource::OwnerPocket => {
            let terminal_id = payment.terminal_id.ok_or_else(|| {
                AppError::validation("terminalId is required to pay from the drawer")
            })?;
            if payment.paid_on.is_some() {
                return Err(AppError::validation(
                    "Cash is booked when it is recorded; remove the payment date.",
                ));
            }
            let own_pocket = payment.source == PaymentSource::OwnerPocket;
            let capital = if own_pocket {
                Some(
                    movement_service::prepare_movement(
                        db,
                        by.user_id.clone(),
                        by.name.clone(),
                        CreateCashMovementRequest {
                            movement_type: CashMovementType::AddCash,
                            amount_cents: payment.amount_cents,
                            session_key: payment.session_key.clone(),
                            terminal_id: terminal_id.clone(),
                            category_key: category_key(CashMovementType::AddCash, "Owner capital"),
                            custom_label: None,
                            payee_key: None,
                            payee_name: None,
                            period_start: None,
                            period_end: None,
                            reference_no: reference_no.map(str::to_string),
                            note: Some(format!("Paid {supplier_name} from own pocket. {note}")),
                            manager_pin: None,
                            override_reason: None,
                            confirm_duplicate: false,
                            settlements: Vec::new(),
                        },
                        device_id.clone(),
                        Some(receipt_key.to_string()),
                        0,
                    )
                    .await?,
                )
            } else {
                None
            };
            let paid = movement_service::prepare_movement(
                db,
                by.user_id,
                by.name,
                CreateCashMovementRequest {
                    movement_type: CashMovementType::SupplierPayment,
                    amount_cents: payment.amount_cents,
                    session_key: payment.session_key,
                    terminal_id,
                    category_key: payment.category_key,
                    custom_label: None,
                    payee_key: Some(supplier_key.to_string()),
                    payee_name: None,
                    period_start: None,
                    period_end: None,
                    reference_no: reference_no.map(str::to_string),
                    note: Some(note),
                    manager_pin: payment.manager_pin,
                    override_reason: payment.override_reason,
                    confirm_duplicate: false,
                    settlements: Vec::new(),
                },
                device_id,
                Some(receipt_key.to_string()),
                if own_pocket { payment.amount_cents } else { 0 },
            )
            .await?;
            Ok(PreparedPayment::Cash(Box::new(CashPayment {
                capital,
                payment: paid,
            })))
        }
        PaymentSource::Bank | PaymentSource::Card | PaymentSource::Online => {
            if payment.terminal_id.is_some() || payment.session_key.is_some() {
                return Err(AppError::validation(
                    "A register terminal or shift only applies to cash from the drawer.",
                ));
            }
            if payment.manager_pin.is_some() || payment.override_reason.is_some() {
                return Err(AppError::validation(
                    "A manager PIN only applies to cash from the drawer.",
                ));
            }
            let paid_on = payment.paid_on.ok_or_else(|| {
                AppError::validation("Say on which day the money left the account")
            })?;
            let paid_from = match payment.source {
                PaymentSource::Bank => PaidFrom::Bank,
                PaymentSource::Card => PaidFrom::Card,
                _ => PaidFrom::Online,
            };
            let prepared = expense_service::prepare_expense(
                db,
                by.user_id,
                by.name,
                CreateExpenseRequest {
                    expense_date: paid_on,
                    category_key: payment.category_key,
                    payee_key: Some(supplier_key.to_string()),
                    payee_name: None,
                    amount_cents: payment.amount_cents,
                    paid_from,
                    due_date: None,
                    reference_no: reference_no.map(str::to_string),
                    note: Some(note),
                    settlements: Vec::new(),
                },
                device_id,
            )
            .await?;
            Ok(PreparedPayment::Ledger(Box::new(prepared)))
        }
    }
}
/// Trims an optional text; blank counts as absent, too long is refused.
fn clean(label: &str, value: Option<String>, max: usize) -> AppResult<Option<String>> {
    match value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        Some(text) if text.chars().count() > max => Err(AppError::validation(format!(
            "{label} must be at most {max} characters"
        ))),
        other => Ok(other),
    }
}

/// Validates the receipt, resolves its supplier and products, then records the header and every
/// line together with the stock each one brings in.
pub async fn record_receipt(
    db: &Db,
    recorded_by: Option<RecordedBy>,
    body: CreateReceiptRequest,
    device_id: Option<String>,
) -> AppResult<PurchaseReceipt> {
    crate::core::logging::domain::tracked("purchases.receipt_recorded", async move {
        if body.lines.is_empty() {
            return Err(AppError::validation(
                "Add at least one product to the receipt",
            ));
        }
        let reference_no = clean("The invoice number", body.reference_no, REFERENCE_MAX)?;
        let notes = clean("The notes", body.notes, NOTES_MAX)?;

        let supplier = supplier_service::get_supplier_by_key(db, &body.supplier_key).await?;

        let mut seen_products = HashSet::new();
        let mut seen_serials = HashSet::new();
        let mut resolved = Vec::with_capacity(body.lines.len());
        for line in &body.lines {
            if line.quantity < 1 {
                return Err(AppError::validation("Quantity must be at least 1"));
            }
            inventory_stock_batch::validate_batch_prices(
                line.unit_cost_cents,
                line.selling_price_cents,
            )?;
            if !seen_products.insert(line.product_key.clone()) {
                return Err(AppError::validation(
                    "A product can be on a receipt only once; raise its quantity instead",
                ));
            }
            let product = inventory_product::get_product_by_key(db, &line.product_key).await?;

            let serials = line.serial_numbers.clone().unwrap_or_default();
            if product.is_serialized {
                if serials.len() as i64 != line.quantity {
                    return Err(AppError::validation(format!(
                        "{} is serialized — provide exactly {} serial number(s), got {}",
                        product.name,
                        line.quantity,
                        serials.len()
                    )));
                }
                for serial in &serials {
                    if !seen_serials.insert(serial.clone()) {
                        return Err(AppError::custom(
                            axum::http::StatusCode::CONFLICT,
                            codes::SERIAL_ALREADY_EXISTS,
                            format!(
                                "Serial number '{serial}' appears more than once on this receipt"
                            ),
                        ));
                    }
                }
                inventory_product_serial::ensure_serials_untracked(db, &serials).await?;
            } else if !serials.is_empty() {
                return Err(AppError::validation(format!(
                    "{} is not serialized — remove its serial numbers",
                    product.name
                )));
            }
            resolved.push((product, serials));
        }

        if let Some(reference) = &reference_no
            && !body.confirm_duplicate
            && let Some(earlier) = repository::receipt::find_receipt_by_supplier_and_reference(
                db,
                &supplier.key,
                reference,
            )
            .await?
        {
            return Err(AppError::conflict_with_details(
                codes::DUPLICATE_RECEIPT,
                format!(
                    "{} already has a receipt with invoice number {reference} ({})",
                    supplier.name, earlier.receipt_number
                ),
                serde_json::json!({
                    "receiptKey": earlier.key,
                    "receiptNumber": earlier.receipt_number,
                    "date": earlier.date.to_chrono(),
                }),
            ));
        }

        // Taken before the transaction opens; a failed save leaves a gap in the numbers.
        let reservation = sequences::service::reserve_sequence(
            db,
            "purchase".to_string(),
            ReserveSequenceRequest {
                block_size: Some(1),
                device_id: device_id.clone(),
            },
        )
        .await?;
        let receipt_number = format!(
            "{}{:0width$}",
            reservation.prefix,
            reservation.start,
            width = reservation.padding
        );

        let now = BsonDateTime::now();
        let date = BsonDateTime::from_chrono(body.date);
        let receipt_key = generate_id(prefixes::PURCHASE_RECEIPT);

        // Checked and prepared before the transaction opens, so a refused payment (the drawer
        // limit, a missing PIN, a bad day) stops the whole receipt before anything is written.
        let total_cents: i64 = body
            .lines
            .iter()
            .map(|line| line.quantity * line.unit_cost_cents)
            .sum();
        let payment = match body.payment {
            Some(payment) => {
                let by = recorded_by.ok_or_else(|| {
                    AppError::internal("A receipt payment needs to know who recorded it")
                })?;
                let amount_cents = payment.amount_cents;
                Some((
                    prepare_payment(
                        db,
                        by,
                        &supplier.key,
                        &supplier.name,
                        &receipt_key,
                        &receipt_number,
                        reference_no.as_deref(),
                        total_cents,
                        payment,
                        device_id,
                    )
                    .await?,
                    amount_cents,
                ))
            }
            None => None,
        };

        let mut tx = db.begin().await?;
        let receipt = repository::receipt::insert_receipt(
            &mut tx,
            PurchaseReceiptDocument {
                id: None,
                key: receipt_key.clone(),
                receipt_number,
                supplier_key: supplier.key.clone(),
                reference_no: reference_no.clone(),
                date,
                notes,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;

        let mut lines = Vec::with_capacity(resolved.len());
        for (line, (product, serials)) in body.lines.iter().zip(resolved) {
            let purchase_key = generate_id(prefixes::PURCHASE);
            let inserted = repository::insert_purchase(
                &mut tx,
                PurchaseDocument {
                    id: None,
                    key: purchase_key.clone(),
                    supplier_key: supplier.key.clone(),
                    product_key: product.key.clone(),
                    quantity: line.quantity,
                    unit_cost_cents: line.unit_cost_cents,
                    selling_price_cents: line.selling_price_cents,
                    date,
                    receipt_key: receipt_key.clone(),
                    version: 1,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                    updated_by_device: None,
                },
            )
            .await?;

            let product_object_id = mongodb::bson::oid::ObjectId::parse_str(&product.id).expect(
                "Product.id returned from get_product_by_key is always a valid ObjectId hex string",
            );
            let batch = inventory_stock_batch::create_batch(
                &mut tx,
                inventory_stock_batch::NewBatch {
                    product_id: product_object_id,
                    product_key: product.key.clone(),
                    source: BatchSource::Purchase,
                    purchase_key: Some(purchase_key.clone()),
                    supplier_key: Some(supplier.key.clone()),
                    quantity: line.quantity,
                    unit_cost_cents: line.unit_cost_cents,
                    selling_price_cents: line.selling_price_cents,
                    received_at: date,
                    movement_type: StockMovementType::PurchaseReceipt,
                    reference_id: Some(purchase_key),
                    note: reference_no.clone(),
                },
            )
            .await?;

            if product.is_serialized {
                inventory_product_serial::create_serials_for_purchase(
                    &mut tx,
                    &product.key,
                    &batch.key,
                    &serials,
                )
                .await?;
            }
            lines.push(inserted);
        }

        if let Some((payment, paid_cents)) = payment {
            let (payment_kind, payment_key) = match payment {
                PreparedPayment::Cash(cash) => {
                    let CashPayment { capital, payment } = *cash;
                    if let Some(capital) = capital {
                        movement_service::write_movement(&mut tx, capital).await?;
                    }
                    let key = payment.key.clone();
                    movement_service::write_movement(&mut tx, payment).await?;
                    (PaymentKind::CashMovement, key)
                }
                PreparedPayment::Ledger(prepared) => {
                    let key = prepared
                        .payment_key()
                        .ok_or_else(|| {
                            AppError::internal("A paid ledger expense must have a payment")
                        })?
                        .to_string();
                    expense_service::write_expense(&mut tx, *prepared).await?;
                    (PaymentKind::ExpensePayment, key)
                }
            };
            repository::settlement::insert_settlement(
                &mut tx,
                ReceiptSettlementDocument {
                    id: None,
                    key: generate_id(prefixes::RECEIPT_SETTLEMENT),
                    receipt_key: receipt_key.clone(),
                    supplier_key: supplier.key.clone(),
                    amount_cents: paid_cents,
                    payment_kind,
                    payment_key,
                    version: 1,
                    created_at: now,
                    updated_at: now,
                    updated_by_device: None,
                },
            )
            .await?;
        }
        tx.commit().await?;

        let mut assembled = assemble(db, vec![(receipt, lines)]).await?;
        Ok(assembled.remove(0))
    })
    .await
}

/// One receipt with its lines.
pub async fn get_receipt(db: &Db, key: &str) -> AppResult<PurchaseReceipt> {
    let document = repository::receipt::find_receipt_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Receipt not found", codes::RECEIPT_NOT_FOUND)
        })?;
    let lines =
        repository::list_purchases_by_receipts(db, std::slice::from_ref(&document.key)).await?;
    Ok(assemble(db, vec![(document, lines)]).await?.remove(0))
}

/// Receipts newest first, optionally one supplier's. With `open` set, only the receipts with
/// something still due (`true`, oldest first, the order they are paid in) or only the settled
/// ones (`false`).
pub async fn list_receipts(db: &Db, query: ReceiptListQuery) -> AppResult<ReceiptListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 500);
    let skip = (page - 1) * limit;

    if let Some(open) = query.open {
        // shortcut: what is due is worked out from payments, so it cannot be filtered in the
        // query; the supplier's receipts are loaded and filtered here. Upgrade to a stored
        // paid-up marker only if a supplier ever has thousands of receipts.
        let (documents, _) = repository::receipt::list_receipts_paginated(
            db,
            query.supplier_key.as_deref(),
            0,
            ALL_RECEIPTS_LIMIT,
        )
        .await?;
        let keys: Vec<String> = documents.iter().map(|d| d.key.clone()).collect();
        let mut lines_by_receipt: HashMap<String, Vec<PurchaseDocument>> = HashMap::new();
        for line in repository::list_purchases_by_receipts(db, &keys).await? {
            lines_by_receipt
                .entry(line.receipt_key.clone())
                .or_default()
                .push(line);
        }
        let paired = documents
            .into_iter()
            .map(|document| {
                let lines = lines_by_receipt.remove(&document.key).unwrap_or_default();
                (document, lines)
            })
            .collect();
        let mut matching: Vec<PurchaseReceipt> = assemble(db, paired)
            .await?
            .into_iter()
            .filter(|receipt| (receipt.due_cents > 0) == open)
            .collect();
        if open {
            matching.sort_by(|a, b| a.date.cmp(&b.date).then(a.created_at.cmp(&b.created_at)));
        }
        let total = matching.len() as u64;
        let total_pages = if total == 0 { 1 } else { total.div_ceil(limit) };
        return Ok(ReceiptListResponse {
            items: matching
                .into_iter()
                .skip(skip as usize)
                .take(limit as usize)
                .collect(),
            pagination: PaginationMeta {
                page,
                limit,
                total,
                total_pages,
            },
        });
    }

    let (documents, total) = repository::receipt::list_receipts_paginated(
        db,
        query.supplier_key.as_deref(),
        skip,
        limit as i64,
    )
    .await?;

    let keys: Vec<String> = documents.iter().map(|d| d.key.clone()).collect();
    let mut lines_by_receipt: HashMap<String, Vec<PurchaseDocument>> = HashMap::new();
    for line in repository::list_purchases_by_receipts(db, &keys).await? {
        lines_by_receipt
            .entry(line.receipt_key.clone())
            .or_default()
            .push(line);
    }
    let paired = documents
        .into_iter()
        .map(|document| {
            let lines = lines_by_receipt.remove(&document.key).unwrap_or_default();
            (document, lines)
        })
        .collect();

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };
    Ok(ReceiptListResponse {
        items: assemble(db, paired).await?,
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        },
    })
}

/// Builds the API receipts from their documents and lines: one batch lookup each for the
/// suppliers and (through `enrich_purchases`) the products.
async fn assemble(
    db: &Db,
    receipts: Vec<(PurchaseReceiptDocument, Vec<PurchaseDocument>)>,
) -> AppResult<Vec<PurchaseReceipt>> {
    let supplier_keys: Vec<String> = receipts
        .iter()
        .map(|(receipt, _)| receipt.supplier_key.clone())
        .collect();
    let suppliers_by_key: HashMap<String, SupplierSummary> =
        supplier_service::get_suppliers_by_keys(db, &supplier_keys)
            .await?
            .into_iter()
            .map(|supplier| {
                (
                    supplier.key.clone(),
                    SupplierSummary {
                        id: supplier.id,
                        key: supplier.key,
                        name: supplier.name,
                        contact_person: supplier.contact_person,
                        primary_phone: supplier.primary_phone,
                    },
                )
            })
            .collect();

    // What was paid against each receipt. A payment that was later reversed is kept in the list
    // but no longer counts as paid.
    let receipt_keys: Vec<String> = receipts.iter().map(|(r, _)| r.key.clone()).collect();
    let settlements =
        repository::settlement::list_settlements_for_receipts(db, &receipt_keys).await?;
    let cash_payment_keys: Vec<String> = settlements
        .iter()
        .filter(|s| s.payment_kind == PaymentKind::CashMovement)
        .map(|s| s.payment_key.clone())
        .collect();
    let reversed = movement_service::reversed_keys(db, &cash_payment_keys).await?;
    let mut settlements_by_receipt: HashMap<String, Vec<ReceiptSettlement>> = HashMap::new();
    for settlement in settlements {
        let is_reversed = settlement.payment_kind == PaymentKind::CashMovement
            && reversed.contains(&settlement.payment_key);
        settlements_by_receipt
            .entry(settlement.receipt_key.clone())
            .or_default()
            .push(settlement.into_settlement(is_reversed));
    }

    let credited = credit_service::credited_by_receipt(db, &receipt_keys).await?;

    let mut assembled = Vec::with_capacity(receipts.len());
    for (receipt, lines) in receipts {
        let lines = enrich_purchases(db, lines).await?;
        let supplier = suppliers_by_key.get(&receipt.supplier_key).cloned();
        let figures = ReceiptFigures {
            credited_cents: credited.get(&receipt.key).copied().unwrap_or(0),
            settlements: settlements_by_receipt
                .remove(&receipt.key)
                .unwrap_or_default(),
        };
        assembled.push(receipt.into_receipt(supplier, lines, figures));
    }
    Ok(assembled)
}

/// Converts raw `purchase_receipts` documents, as the sync module reads them, into the wire
/// shape. Only the stored fields matter to the receiver: the supplier, the lines and the total
/// are computed on read, and the receiving side drops them.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<PurchaseReceipt>> {
    documents
        .into_iter()
        .map(|document| {
            let receipt = bson::deserialize_from_document::<PurchaseReceiptDocument>(document)
                .map_err(AppError::from)?;
            Ok(receipt.into_receipt(
                None,
                Vec::new(),
                ReceiptFigures {
                    credited_cents: 0,
                    settlements: Vec::new(),
                },
            ))
        })
        .collect()
}
