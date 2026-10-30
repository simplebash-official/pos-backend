use super::to_movements::to_movements;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::{CashMovementListQuery, CashMovementListResponse};

use crate::modules::cash_categories;
use crate::modules::sessions::repository::movements;

/// A page of movements, newest first. Filtering by a category includes everything merged into it.
pub async fn list_movements(
    db: &Db,
    mut query: CashMovementListQuery,
) -> AppResult<CashMovementListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);
    if let Some(key) = query.category_key.take() {
        query.category_keys =
            Some(cash_categories::service::keys_including_merged(db, &key).await?);
    }
    let (docs, total) = movements::list_cash_movements(db, &query).await?;
    let total_pages = total.div_ceil(limit);

    Ok(CashMovementListResponse {
        movements: to_movements(db, docs).await?,
        total,
        page,
        limit,
        total_pages,
    })
}
