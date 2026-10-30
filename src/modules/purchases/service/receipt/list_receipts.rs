use super::assemble::assemble;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::PaginationMeta;
use crate::domain::purchase_receipts::{PurchaseReceipt, ReceiptListQuery, ReceiptListResponse};

use crate::modules::purchases::model::PurchaseDocument;

use crate::modules::purchases::repository;

use std::collections::HashMap;

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
