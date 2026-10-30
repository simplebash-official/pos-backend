use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::inventory::PaginationMeta;
use crate::domain::purchase_receipts::{
    PaymentKind, PurchaseReceipt, ReceiptCharge, ReceiptListQuery, ReceiptListResponse,
    ReceiptSettlement,
};
use crate::domain::purchases::SupplierSummary;
use crate::modules::purchases::model::{
    PurchaseDocument, PurchaseReceiptDocument, ReceiptChargeDocument, ReceiptFigures,
};
use crate::modules::purchases::repository;
use crate::modules::purchases::service::credit as credit_service;
use crate::modules::purchases::service::purchase::enrich_purchases;
use crate::modules::sessions::service::movement as movement_service;
use crate::modules::suppliers::service as supplier_service;
use std::collections::HashMap;

/// Builds the API receipts from their documents and lines: one batch lookup each for the
/// suppliers and (through `enrich_purchases`) the products.
pub(super) async fn assemble(
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

    let mut charges_by_receipt: HashMap<String, Vec<ReceiptCharge>> = HashMap::new();
    for charge in repository::charge::list_charges_for_receipts(db, &receipt_keys).await? {
        charges_by_receipt
            .entry(charge.receipt_key.clone())
            .or_default()
            .push(charge.into_charge());
    }

    let mut assembled = Vec::with_capacity(receipts.len());
    for (receipt, lines) in receipts {
        let lines = enrich_purchases(db, lines).await?;
        let supplier = suppliers_by_key.get(&receipt.supplier_key).cloned();
        let figures = ReceiptFigures {
            credited_cents: credited.get(&receipt.key).copied().unwrap_or(0),
            settlements: settlements_by_receipt
                .remove(&receipt.key)
                .unwrap_or_default(),
            charges: charges_by_receipt.remove(&receipt.key).unwrap_or_default(),
        };
        assembled.push(receipt.into_receipt(supplier, lines, figures));
    }
    Ok(assembled)
}

pub(crate) const CHARGE_DESCRIPTION_MAX: usize = 80;

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

/// Converts raw `receipt_charges` documents, as the sync module reads them, into the wire shape.
pub(crate) fn hydrate_charge_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<ReceiptCharge>> {
    documents
        .into_iter()
        .map(|document| {
            bson::deserialize_from_document::<ReceiptChargeDocument>(document)
                .map(ReceiptChargeDocument::into_charge)
                .map_err(AppError::from)
        })
        .collect()
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
                    charges: Vec::new(),
                },
            ))
        })
        .collect()
}

/// Upper bound when every receipt of a supplier has to be looked at.
const ALL_RECEIPTS_LIMIT: i64 = 10_000;

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

/// Who is recording the receipt, for the payment made with it.
#[derive(Debug)]
pub struct RecordedBy {
    pub user_id: String,
    pub name: String,
}

pub(crate) const REFERENCE_MAX: usize = 60;
