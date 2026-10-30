use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::PaginationMeta;
use crate::domain::supplier_credits::{SupplierCreditListQuery, SupplierCreditListResponse};

use crate::modules::purchases::repository;

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
