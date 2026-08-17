// Business rules and orchestration for the billing module. `sale` owns
// complete-sale and cancel (the money-moving flows, D3/D4/D8 in the
// migration plan); `payments` owns standalone payment recording (partial
// credit repayments); `print_payload` builds the Typst data contract
// consumed by `modules::documents::service`. Read paths (`get_invoice`/
// `list_invoices`) live directly here since they're straightforward
// lookups, not orchestration.

pub mod payments;
pub(crate) mod print_payload;
pub mod sale;

use mongodb::{
    Database,
    bson::{Document, doc},
};

use crate::{
    core::{
        constants::codes,
        error::{AppError, AppResult},
        utils::{build_bson_regex, calculate_pagination},
    },
    domain::billing::{Invoice, InvoiceListQuery, InvoiceListResponse},
    modules::billing::repository,
};

pub async fn get_invoice(db: &Database, id_or_key: &str) -> AppResult<Invoice> {
    let document = repository::find_invoice_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;
    Ok(document.into_invoice())
}

pub async fn list_invoices(
    db: &Database,
    query: InvoiceListQuery,
) -> AppResult<InvoiceListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        and_clauses.push(doc! {
            "$or": [
                { "invoice_number": { "$regex": pattern.clone() } },
                { "customer_name_snapshot": { "$regex": pattern.clone() } },
                { "customer_phone_snapshot": { "$regex": pattern } },
            ]
        });
    }
    if let Some(status) = query.status.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "status": status.trim() });
    }
    if let Some(customer_key) = query.customer_key.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "customer_key": customer_key.trim() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let (documents, total) = repository::list_invoices(db, filter, skip, limit).await?;
    let invoices = documents.into_iter().map(|d| d.into_invoice()).collect();
    let total_pages = total.div_ceil(limit);

    Ok(InvoiceListResponse {
        invoices,
        total,
        page,
        limit,
        total_pages,
    })
}
