use super::names_are_similar::names_are_similar;
use super::usage_by_key::usage_by_key;
use super::usage_of::usage_of;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::{CashCategory, SimilarCashCategoryQuery, normalize_name};
use crate::domain::sessions::CashMovementType;
use crate::modules::cash_categories::repository;

/// Existing categories whose name is close to a typed one, so the cashier can use one instead of adding a near-duplicate.
pub async fn similar_categories(
    db: &Db,
    query: SimilarCashCategoryQuery,
) -> AppResult<Vec<CashCategory>> {
    let movement_type = CashMovementType::parse(&query.movement_type)
        .ok_or_else(|| AppError::validation("Unknown movement type."))?;
    let typed = normalize_name(&query.name);
    if typed.is_empty() {
        return Ok(Vec::new());
    }
    let all = repository::list_all(db).await?;
    let usage = usage_by_key(db).await?;
    let mut hits: Vec<CashCategory> = all
        .iter()
        .filter(|c| c.movement_type == movement_type && c.active && c.merged_into_key.is_none())
        .filter(|c| names_are_similar(&typed, &c.normalized_name))
        .cloned()
        .map(|c| {
            let n = usage_of(&c, &all, &usage);
            c.into_cash_category(n)
        })
        .collect();
    hits.sort_by_key(|c| std::cmp::Reverse(c.usage_count));
    hits.truncate(3);
    Ok(hits)
}
