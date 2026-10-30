use super::today::today;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::core::utils::calculate_pagination;

use crate::domain::quotations::{QuotationListQuery, QuotationListResponse};

use crate::modules::quotations::repository;
use crate::modules::quotations::repository::ListFilter;

const LIST_STATUSES: &[&str] = &["active", "expired", "cancelled"];

pub async fn list_quotations(
    db: &Db,
    query: QuotationListQuery,
) -> AppResult<QuotationListResponse> {
    let status = query
        .status
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(s) = &status
        && !LIST_STATUSES.contains(&s.as_str())
    {
        return Err(AppError::validation(format!(
            "Invalid status '{s}'. Must be one of: {}",
            LIST_STATUSES.join(", ")
        )));
    }
    let search = query
        .search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let today = today();
    let filter = ListFilter {
        search,
        status,
        today: today.clone(),
    };
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let (documents, total) = repository::list(db, &filter, skip, limit).await?;

    Ok(QuotationListResponse {
        quotations: documents
            .into_iter()
            .map(|d| d.into_quotation(&today))
            .collect(),
        total,
        page,
        limit,
        total_pages: total.div_ceil(limit),
    })
}
