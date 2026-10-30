use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::utils::{build_bson_regex, calculate_pagination, shop_today_range};
use crate::domain::billing::{CreditNoteListQuery, CreditNoteListResponse};

use crate::modules::billing::repository;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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
