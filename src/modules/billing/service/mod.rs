// Business rules and orchestration for the billing module. `sale` owns
// complete-sale and cancel (the money-moving flows, D3/D4/D8 in the
// migration plan); `payments` owns standalone payment recording (partial
// credit repayments); `print_payload` builds the Typst data contract
// consumed by `modules::documents::service`. Read paths (`get_invoice`/
// `list_invoices`) live directly here since they're straightforward
// lookups, not orchestration.

pub mod payments;
pub(crate) mod print_payload;
pub mod returns;
pub mod sale;

use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::{
    core::{
        constants::codes,
        error::{AppError, AppResult},
        utils::{build_bson_regex, calculate_pagination, today_utc_range},
    },
    domain::billing::{BillingStats, Invoice, InvoiceListQuery, InvoiceListResponse},
    modules::billing::{model::InvoiceDocument, repository},
};

/// Invoices are append-only (no `deleted_at`, no edit/delete route — see
/// `InvoiceDocument`'s doc comment), so unlike every other synced resource
/// this hydrate never needs to worry about tombstones reaching the client.
/// See `repairs::service::hydrate_sync_documents` for why delta and snapshot
/// feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<Invoice>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<InvoiceDocument>(document)?.into_invoice())
        })
        .collect()
}

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
    if let Some(payment_status) = query.payment_status.filter(|s| !s.trim().is_empty()) {
        match payment_status.trim() {
            "paid" => and_clauses.push(doc! { "status": "paid", "is_credit": false }),
            "credit" => {
                and_clauses.push(doc! { "$or": [ { "is_credit": true }, { "status": "pending" } ] })
            }
            _ => {}
        }
    }
    if let Some(payment_method) = query.payment_method.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "payment_method": payment_method.trim() });
    }
    if query.date_preset.as_deref() == Some("today") {
        let (today_start, today_end) = today_utc_range();
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

/// Computes the 4 dashboard KPI cards for the Sales & Invoices History
/// screen. See `today_utc_range` for how "today" is bounded.
pub async fn get_billing_stats(db: &Database) -> AppResult<BillingStats> {
    let (today_start, today_end) = today_utc_range();
    let agg = repository::aggregate_stats(db, today_start, today_end).await?;

    let avg_basket_cents = if agg.today_invoice_count > 0 {
        (agg.today_sales_cents as f64 / agg.today_invoice_count as f64).round() as i64
    } else {
        0
    };

    Ok(BillingStats {
        today_sales_cents: agg.today_sales_cents,
        today_invoice_count: agg.today_invoice_count,
        outstanding_credit_cents: agg.outstanding_credit_cents,
        avg_basket_cents,
    })
}
