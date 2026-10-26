// Business rules and orchestration for the billing module. `sale` owns
// complete-sale, void, and close (the money-moving flows, D3/D4/D8 in the
// migration plan); `payments` owns standalone payment recording (partial
// credit repayments); `credit_notes` owns returns/refunds/exchanges;
// `print_payload` builds the Typst data contract consumed by
// `modules::documents::service`. Read paths (`get_invoice`/`list_invoices`)
// live directly here since they're straightforward lookups, not
// orchestration.

pub mod credit_notes;
pub mod payments;
pub(crate) mod print_payload;
pub mod sale;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

use crate::{
    clients::db::Db,
    core::{
        constants::codes,
        error::{AppError, AppResult},
        utils::{build_bson_regex, calculate_pagination, shop_today_range},
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

pub async fn get_invoice(db: &Db, id_or_key: &str) -> AppResult<Invoice> {
    let document = repository::find_invoice_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;
    Ok(document.into_invoice())
}

pub async fn list_invoices(db: &Db, query: InvoiceListQuery) -> AppResult<InvoiceListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        let mut or_clauses = vec![
            doc! { "invoice_number": { "$regex": pattern.clone() } },
            doc! { "customer_name_snapshot": { "$regex": pattern.clone() } },
            doc! { "customer_phone_snapshot": { "$regex": pattern } },
        ];

        let digits: String = search.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digits.parse::<u64>() {
            let padded = format!("INV-{num:06}");
            or_clauses.push(doc! { "invoice_number": padded });
        }

        and_clauses.push(doc! { "$or": or_clauses });
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
    if let Some(session_key) = query.session_key.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "session_key": session_key.trim() });
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
/// screen. See `shop_today_range` for how "today" is bounded.
pub async fn get_billing_stats(db: &Db) -> AppResult<BillingStats> {
    let (today_start, today_end) = shop_today_range();
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

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionBillingSummary {
    pub total_invoices_count: u64,
    pub discount_cents: i64,
    pub cash_payment_count: u64,
    pub card_payment_count: u64,
    pub online_payment_count: u64,
    pub credit_invoice_count: u64,
}

/// Aggregates sales invoice counts, discounts, and tender transaction counts for a register session.
pub(crate) async fn get_session_billing_summary(
    db: &Db,
    session_key: &str,
) -> AppResult<SessionBillingSummary> {
    let (invoices, payments) =
        repository::find_session_invoices_and_payments(db, session_key).await?;

    let total_invoices_count = invoices.len() as u64;
    let discount_cents = invoices.iter().map(|inv| inv.discount_cents).sum();
    let credit_invoice_count = invoices
        .iter()
        .filter(|inv| inv.is_credit || inv.payment_method == "credit")
        .count() as u64;

    let mut cash_payment_count = 0u64;
    let mut card_payment_count = 0u64;
    let mut online_payment_count = 0u64;

    for payment in &payments {
        match payment.payment_method.as_str() {
            "cash" => cash_payment_count += 1,
            "card" => card_payment_count += 1,
            "online" => online_payment_count += 1,
            _ => {}
        }
    }

    Ok(SessionBillingSummary {
        total_invoices_count,
        discount_cents,
        cash_payment_count,
        card_payment_count,
        online_payment_count,
        credit_invoice_count,
    })
}
