use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::core::utils::{build_bson_regex, calculate_pagination, shop_today_range};
use crate::domain::billing::{InvoiceListQuery, InvoiceListResponse};

use crate::modules::billing::repository;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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
