use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::core::utils::calculate_pagination;
use crate::domain::imports::{ImportBatchListQuery, ImportBatchListResponse};
use crate::domain::inventory::PaginationMeta;
use crate::modules::imports::model::ImportBatchDocument;
use crate::modules::imports::repository;

pub(crate) async fn list_import_batches(
    db: &Db,
    query: ImportBatchListQuery,
) -> AppResult<ImportBatchListResponse> {
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);
    let (docs, total) =
        repository::batch::list_batches(db, query.target.as_deref(), skip, limit).await?;

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };

    let pagination = PaginationMeta {
        page,
        limit,
        total,
        total_pages,
    };

    let items = docs
        .into_iter()
        .map(ImportBatchDocument::into_import_batch)
        .collect();

    Ok(ImportBatchListResponse { items, pagination })
}
