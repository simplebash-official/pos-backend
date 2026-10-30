use super::all_rows::all_rows;
use super::parse_day::parse_day;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::normalize_name;
use crate::domain::incomes::{IncomeListQuery, IncomeListResponse, IncomeRow, IncomeSource};
use crate::modules::cash_categories;

/// A page of income with the filters of the Income screen.
pub async fn list_incomes(db: &Db, query: IncomeListQuery) -> AppResult<IncomeListResponse> {
    if let Some(from) = &query.from {
        parse_day("start date", from)?;
    }
    if let Some(to) = &query.to {
        parse_day("end date", to)?;
    }
    let source = match query.source.as_deref() {
        None => None,
        Some("ledger") => Some(IncomeSource::Ledger),
        Some("drawer") => Some(IncomeSource::Drawer),
        Some(_) => return Err(AppError::validation("Unknown source.")),
    };
    let category_keys = match &query.category_key {
        Some(key) => Some(cash_categories::service::keys_including_merged(db, key).await?),
        None => None,
    };
    let search = query
        .search
        .as_deref()
        .map(normalize_name)
        .filter(|s| !s.is_empty());

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);

    let rows: Vec<IncomeRow> = all_rows(db, query.from.as_deref(), query.to.as_deref())
        .await?
        .into_iter()
        .filter(|r| source.is_none_or(|s| r.source == s))
        .filter(|r| {
            category_keys
                .as_ref()
                .is_none_or(|keys| keys.contains(&r.category_key))
        })
        .filter(|r| {
            query
                .payee_key
                .as_ref()
                .is_none_or(|k| r.payee_key.as_ref() == Some(k))
        })
        .filter(|r| {
            search.as_ref().is_none_or(|q| {
                [
                    Some(r.number.as_str()),
                    Some(r.category_name.as_str()),
                    r.payee_name.as_deref(),
                    r.reference_no.as_deref(),
                    r.note.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|text| normalize_name(text).contains(q.as_str()))
            })
        })
        .collect();

    let total = rows.len() as u64;
    let start = ((page - 1) * limit) as usize;
    Ok(IncomeListResponse {
        incomes: rows.into_iter().skip(start).take(limit as usize).collect(),
        total,
        page,
        limit,
        total_pages: total.div_ceil(limit),
    })
}
