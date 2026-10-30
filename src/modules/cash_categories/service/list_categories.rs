use super::is_expense_group::is_expense_group;
use super::usage_by_key::usage_by_key;
use super::usage_of::usage_of;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::{
    CashCategory, CashCategoryListQuery, ReportGroup, ReviewStatus, normalize_name,
};
use crate::domain::sessions::CashMovementType;
use crate::modules::cash_categories::repository;

/// Categories for the picker and the management screen, best first: pinned, then most
/// used by this shop, then the shop's own order. Merged-away categories never appear.
pub async fn list_categories(
    db: &Db,
    query: CashCategoryListQuery,
) -> AppResult<Vec<CashCategory>> {
    let movement_type = query
        .movement_type
        .as_deref()
        .map(|t| {
            CashMovementType::parse(t).ok_or_else(|| AppError::validation("Unknown movement type."))
        })
        .transpose()?;
    let include_hidden = query.include_hidden.unwrap_or(false);
    let needs_review = query.needs_review.unwrap_or(false);
    let only_costs = query.for_expenses.unwrap_or(false);
    let only_income = query.for_income.unwrap_or(false);
    let search = query
        .search
        .as_deref()
        .map(normalize_name)
        .filter(|s| !s.is_empty());

    let all = repository::list_all(db).await?;
    let usage = usage_by_key(db).await?;

    let mut out: Vec<CashCategory> = all
        .iter()
        .filter(|c| c.merged_into_key.is_none())
        .filter(|c| movement_type.is_none_or(|t| c.movement_type == t))
        .filter(|c| include_hidden || c.active)
        .filter(|c| !needs_review || c.review_status == ReviewStatus::NeedsReview)
        .filter(|c| !only_costs || is_expense_group(c.report_group))
        .filter(|c| !only_income || c.report_group == ReportGroup::OtherIncome)
        .filter(|c| {
            search.as_ref().is_none_or(|q| {
                c.normalized_name.contains(q.as_str())
                    || c.name_si
                        .as_deref()
                        .is_some_and(|si| normalize_name(si).contains(q.as_str()))
            })
        })
        .cloned()
        .map(|c| {
            let n = usage_of(&c, &all, &usage);
            c.into_cash_category(n)
        })
        .collect();
    out.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.usage_count.cmp(&a.usage_count))
            .then(a.sort_order.cmp(&b.sort_order))
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}
