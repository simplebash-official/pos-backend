// Business rules for supplier credits: an amount a supplier takes off what the shop owes, for
// short or damaged goods or a price difference. A credit against a receipt can never be more than
// that receipt still owes. Credits move no stock. A wrong credit is voided by a reversing entry
// whose key is derived from the credit's, so two devices voiding the same credit offline write
// the same row; nothing is ever edited or deleted.

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
        inventory::PaginationMeta,
        supplier_credits::{
            CreateSupplierCreditRequest, SupplierCredit, SupplierCreditListQuery,
            SupplierCreditListResponse,
        },
    },
    modules::{
        purchases::{
            model::SupplierCreditDocument,
            repository,
            service::receipt::{RecordedBy, get_receipt},
        },
        suppliers::service as supplier_service,
    },
};

const NOTE_MAX: usize = 200;

/// Records a credit from a supplier. Against a receipt, it may not exceed what that receipt still
/// owes; with no receipt it is credit on the supplier's account.
pub async fn create_credit(
    db: &Db,
    by: RecordedBy,
    body: CreateSupplierCreditRequest,
) -> AppResult<SupplierCredit> {
    crate::core::logging::domain::tracked("purchases.credit_recorded", async move {
        if body.amount_cents <= 0 {
            return Err(AppError::validation("The credit must be more than zero"));
        }
        let note = body
            .note
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());
        if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_MAX) {
            return Err(AppError::validation(format!(
                "The note must be at most {NOTE_MAX} characters"
            )));
        }
        let supplier = supplier_service::get_supplier_by_key(db, &body.supplier_key).await?;
        if let Some(receipt_key) = &body.receipt_key {
            let receipt = get_receipt(db, receipt_key).await?;
            if receipt.supplier_key != supplier.key {
                return Err(AppError::validation(
                    "That receipt belongs to another supplier",
                ));
            }
            if body.amount_cents > receipt.due_cents {
                return Err(AppError::conflict(
                    codes::RECEIPT_OVERPAID,
                    "That is more than the receipt still owes.",
                ));
            }
        }

        let now = BsonDateTime::now();
        let inserted = repository::credit::insert_credit(
            db,
            SupplierCreditDocument {
                id: None,
                key: generate_id(prefixes::SUPPLIER_CREDIT),
                supplier_key: supplier.key,
                receipt_key: body.receipt_key,
                amount_cents: body.amount_cents,
                reason: body.reason,
                note,
                reverses_key: None,
                recorded_by_user_id: by.user_id,
                recorded_by_name_snapshot: by.name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        Ok(inserted.into_credit(None))
    })
    .await
}

/// Cancels a credit by recording its opposite. Voiding twice is refused.
pub async fn void_credit(db: &Db, by: RecordedBy, key: &str) -> AppResult<SupplierCredit> {
    crate::core::logging::domain::tracked("purchases.credit_voided", async move {
        let original = repository::credit::find_credit_by_key(db, key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Credit not found", codes::CREDIT_NOT_FOUND)
            })?;
        if original.reverses_key.is_some() {
            return Err(AppError::validation_with_code(
                "A voiding entry cannot be voided.",
                codes::CREDIT_NOT_VOIDABLE,
            ));
        }
        let void_key = format!("{key}_rev");
        if repository::credit::find_credit_by_key(db, &void_key)
            .await?
            .is_some()
        {
            return Err(AppError::conflict(
                codes::CREDIT_ALREADY_VOIDED,
                "This credit was already voided.",
            ));
        }
        let now = BsonDateTime::now();
        let inserted = repository::credit::insert_credit(
            db,
            SupplierCreditDocument {
                id: None,
                key: void_key,
                supplier_key: original.supplier_key,
                receipt_key: original.receipt_key,
                amount_cents: original.amount_cents,
                reason: original.reason,
                note: Some(format!("Voids credit {key}")),
                reverses_key: Some(original.key),
                recorded_by_user_id: by.user_id,
                recorded_by_name_snapshot: by.name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        Ok(inserted.into_credit(None))
    })
    .await
}

/// Credits newest first, optionally one supplier's and/or one receipt's.
pub async fn list_credits(
    db: &Db,
    query: SupplierCreditListQuery,
) -> AppResult<SupplierCreditListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 500);
    let skip = (page - 1) * limit;
    let (documents, total) = repository::credit::list_credits_paginated(
        db,
        query.supplier_key.as_deref(),
        query.receipt_key.as_deref(),
        skip,
        limit as i64,
    )
    .await?;

    // Which of these have been voided: look for the voiding entries by their derived keys.
    let mut items = Vec::with_capacity(documents.len());
    for document in documents {
        let voided_by = if document.reverses_key.is_none() {
            let void_key = format!("{}_rev", document.key);
            repository::credit::find_credit_by_key(db, &void_key)
                .await?
                .map(|v| v.key)
        } else {
            None
        };
        items.push(document.into_credit(voided_by));
    }
    let total_pages = if total == 0 { 1 } else { total.div_ceil(limit) };
    Ok(SupplierCreditListResponse {
        items,
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        },
    })
}

/// What is currently credited against each receipt: its credits, with voided ones left out.
pub(crate) async fn credited_by_receipt(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<HashMap<String, i64>> {
    let rows = repository::credit::list_credits_for_receipts(db, receipt_keys).await?;
    Ok(live_credits(&rows)
        .into_iter()
        .filter_map(|c| c.receipt_key.clone().map(|key| (key, c.amount_cents)))
        .fold(HashMap::new(), |mut totals, (key, amount)| {
            *totals.entry(key).or_insert(0) += amount;
            totals
        }))
}

/// A supplier's live credits as `(YYYY-MM-DD, amount in cents)`, for the supplier balance.
pub(crate) async fn credits_of_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<(String, i64)>> {
    let rows = repository::credit::list_credits_for_supplier(db, supplier_key).await?;
    Ok(live_credits(&rows)
        .into_iter()
        .map(|c| {
            (
                c.created_at.to_chrono().format("%Y-%m-%d").to_string(),
                c.amount_cents,
            )
        })
        .collect())
}

/// The credits that still count: not voiding entries, and not voided by one.
fn live_credits(rows: &[SupplierCreditDocument]) -> Vec<&SupplierCreditDocument> {
    let voided: HashSet<&str> = rows
        .iter()
        .filter_map(|r| r.reverses_key.as_deref())
        .collect();
    rows.iter()
        .filter(|r| r.reverses_key.is_none() && !voided.contains(r.key.as_str()))
        .collect()
}

/// Converts raw `supplier_credits` documents, as the sync module reads them, into the wire shape.
/// `voidedByKey` is worked out when credits are listed and is dropped by the receiver.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<SupplierCredit>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<SupplierCreditDocument>(document)
                    .map_err(AppError::from)?
                    .into_credit(None),
            )
        })
        .collect()
}
