use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::core::utils::calculate_pagination;
use crate::domain::inventory::PaginationMeta;
use crate::domain::labels::{LabelJobListQuery, LabelJobListResponse};

use crate::modules::labels::model::LabelPrintJobDocument;
use crate::modules::labels::repository;

use chrono::{DateTime, Utc};
use mongodb::bson::DateTime as BsonDateTime;

fn parse_instant(value: Option<&str>, field: &str) -> AppResult<Option<BsonDateTime>> {
    value
        .filter(|v| !v.is_empty())
        .map(|v| {
            DateTime::parse_from_rfc3339(v)
                .map(|at| BsonDateTime::from_chrono(at.with_timezone(&Utc)))
                .map_err(|_| AppError::validation(format!("'{field}' must be a date and time.")))
        })
        .transpose()
}

/// A page of the print history, newest first.
pub(crate) async fn list_jobs(
    db: &Db,
    query: LabelJobListQuery,
) -> AppResult<LabelJobListResponse> {
    let filter = repository::JobFilter {
        from: parse_instant(query.from.as_deref(), "from")?,
        to: parse_instant(query.to.as_deref(), "to")?,
        product_key: query.product_key.filter(|k| !k.is_empty()),
        action: query.action,
    };
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);
    let (jobs, total) = repository::list_jobs(db, &filter, skip, limit).await?;
    Ok(LabelJobListResponse {
        items: jobs
            .into_iter()
            .map(LabelPrintJobDocument::into_job)
            .collect(),
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages: total.div_ceil(limit),
        },
    })
}
