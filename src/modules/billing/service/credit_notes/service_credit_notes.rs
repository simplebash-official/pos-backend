use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::utils::{build_bson_regex, calculate_pagination, shop_today_range};
use crate::domain::billing::{
    CreditNote, CreditNoteListQuery, CreditNoteListResponse, CreditNoteStatus, InvoiceItem,
    ItemCondition, VoidCreditNoteRequest,
};
use crate::domain::inventory::StockMovementType;
use crate::modules::billing::model::{CreditNoteDocument, PaymentDocument};
use crate::modules::billing::repository;
use crate::modules::inventory;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

/// Look up a single credit note by its hex ObjectId or unique model key (`cn_...`).
pub async fn get_credit_note(db: &Db, id_or_key: &str) -> AppResult<CreditNote> {
    let document = repository::credit_notes::find_credit_note_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Credit note not found", codes::CREDIT_NOTE_NOT_FOUND)
        })?;
    Ok(document.into_credit_note())
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

/// What an invoice has been paid so far, one entry per payment (method, cents), refunds being
/// negative. The refund preview caps the cash refund at their sum and splits it across them.
pub async fn invoice_payment_legs(db: &Db, invoice_key: &str) -> AppResult<Vec<(String, i64)>> {
    Ok(repository::list_payments_for_invoice(db, invoice_key)
        .await?
        .into_iter()
        .map(|p| (p.payment_method, p.amount_cents))
        .collect())
}

/// Which of several lines of the same product a returned item means — a
/// product is on one line per price it was sold at.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LineHint<'a> {
    /// The price the returned unit was sold at (what a new return names).
    pub unit_price_cents: Option<i64>,
    /// The batches the returned units went back into (what a stored credit
    /// note carries); always a subset of the sold line's own batches.
    pub batch_keys: &'a [&'a str],
}

/// Finds the invoice line a returned item refers to: product key (narrowed by
/// `hint` when the product is on more than one line), then source ticket key,
/// then case-insensitive name; a sole line with a sole returned item always
/// matches. Shared with sync's derived-field recompute so a replicated credit
/// note updates the same line a local one would.
pub(crate) fn line_match(
    items: &[InvoiceItem],
    product_key: Option<&str>,
    source_ticket_key: Option<&str>,
    name: Option<&str>,
    hint: LineHint,
    sole_line_and_sole_return: bool,
) -> Option<usize> {
    items.iter().position(|item| {
        if let (Some(req_pk), Some(item_pk)) = (product_key, item.product_key.as_deref())
            && req_pk == item_pk
        {
            let price_matches = hint
                .unit_price_cents
                .is_none_or(|price| price == item.unit_price_cents);
            let batches_match = hint.batch_keys.iter().all(|key| {
                item.batch_allocations
                    .iter()
                    .any(|allocation| allocation.batch_key == *key)
            });
            return price_matches && batches_match;
        }
        if let (Some(req_stk), Some(item_stk)) =
            (source_ticket_key, item.source_ticket_key.as_deref())
            && req_stk == item_stk
        {
            return true;
        }
        if let Some(req_name) = name
            && req_name.eq_ignore_ascii_case(&item.name)
        {
            return true;
        }
        sole_line_and_sole_return
    })
}

/// List credit notes matching query filters with pagination.
pub async fn list_credit_notes(
    db: &Db,
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
        let (today_start, today_end) = shop_today_range();
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
