use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::utils::{build_bson_regex, calculate_pagination, shop_today_range};
use crate::domain::repairs::{RepairListQuery, RepairListResponse};

use crate::modules::repairs::repository;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

pub async fn list_repairs(db: &Db, query: RepairListQuery) -> AppResult<RepairListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        let mut or_clauses = vec![
            doc! { "ticket_number": { "$regex": pattern.clone() } },
            doc! { "customer_name": { "$regex": pattern.clone() } },
            doc! { "customer_phone": { "$regex": pattern.clone() } },
            doc! { "device_model": { "$regex": pattern } },
        ];

        let digits: String = search.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digits.parse::<u64>() {
            let padded_ticket = format!("REP-{num:06}");
            or_clauses.push(doc! { "ticket_number": padded_ticket });
        }

        and_clauses.push(doc! { "$or": or_clauses });
    }
    if let Some(status) = query.status.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "status": status.trim() });
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
    let (documents, total) = repository::list(db, filter, skip, limit).await?;
    let repairs = documents.into_iter().map(|d| d.into_repair()).collect();
    let total_pages = total.div_ceil(limit);

    Ok(RepairListResponse {
        repairs,
        total,
        page,
        limit,
        total_pages,
    })
}
