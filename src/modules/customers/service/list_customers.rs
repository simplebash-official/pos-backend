use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::utils::{build_bson_regex, calculate_pagination};
use crate::domain::customers::{CustomerListQuery, CustomerListResponse};

use crate::modules::customers::repository;

use mongodb::bson::{Document, doc};

fn sort_field_for(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("name") => "name",
        Some("primaryPhone") => "primary_phone",
        Some("outstandingBalanceCents") => "outstanding_balance_cents",
        Some("totalPurchasesCents") => "total_purchases_cents",
        Some("createdAt") => "created_at",
        Some("updatedAt") => "updated_at",
        _ => "updated_at",
    }
}

pub async fn list_customers(db: &Db, query: CustomerListQuery) -> AppResult<CustomerListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "primary_phone": { "$regex": pattern.clone() } },
                { "secondary_phone": { "$regex": pattern.clone() } },
                { "email": { "$regex": pattern.clone() } },
                { "address": { "$regex": pattern.clone() } },
                { "contact_person": { "$regex": pattern } },
            ]
        });
    }

    if let Some(tag) = query.tag.filter(|t| !t.trim().is_empty()) {
        and_clauses.push(doc! { "tags": tag.trim() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let sort_field = sort_field_for(query.sort_by.as_deref());
    let sort_order = if query.sort_order.as_deref() == Some("asc")
        || (query.sort_by.as_deref() == Some("name") && query.sort_order.is_none())
    {
        1
    } else {
        -1
    };
    let sort = doc! { sort_field: sort_order };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 10, 100);

    let (documents, total) = repository::list_customers(db, filter, sort, skip, limit).await?;
    let customers = documents.into_iter().map(|d| d.into_customer()).collect();
    let total_pages = total.div_ceil(limit);

    Ok(CustomerListResponse {
        customers,
        total,
        page,
        limit,
        total_pages,
    })
}
