// Business rules for the income ledger: what can be entered here, and the combined list of
// ledger incomes and drawer add-cash movements.

use chrono::{Months, NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        logging::domain::tracked,
    },
    domain::{
        cash_categories::{ReportGroup, normalize_name},
        incomes::{
            CreateIncomeRequest, IncomeListQuery, IncomeListResponse, IncomeRow, IncomeSource,
            IncomeStats, ReceivedInto,
        },
        sequences::ReserveSequenceRequest,
    },
    modules::{
        cash_categories::{self, service::ResolvedCategory},
        incomes::{model::IncomeDocument, repository},
        payees, sequences,
        sessions::{self, model::CashMovementDocument},
    },
};

const TEXT_MAX: usize = 200;
const REFERENCE_MAX: usize = 60;
const NAME_MAX: usize = 60;

fn parse_day(label: &str, value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("Please pick a valid {label}.")))
}

fn clean(label: &str, value: Option<&str>, max: usize) -> AppResult<Option<String>> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > max {
        return Err(AppError::validation(format!(
            "{label} can have up to {max} letters."
        )));
    }
    Ok(Some(text.to_string()))
}

/// A ledger income as a list row.
fn ledger_row(i: &IncomeDocument, category: &ResolvedCategory) -> IncomeRow {
    IncomeRow {
        key: i.key.clone(),
        source: IncomeSource::Ledger,
        number: i.income_number.clone(),
        date: i.income_date.clone(),
        category_key: i.category_key.clone(),
        category_name: category.name.clone(),
        report_group: category.report_group,
        payee_key: i.payee_key.clone(),
        payee_name: i.payee_name.clone(),
        amount_cents: i.amount_cents,
        received_into: i.received_into,
        reference_no: i.reference_no.clone(),
        note: i.note.clone(),
        recorded_by_name_snapshot: i.recorded_by_name_snapshot.clone(),
        created_at: i.created_at.to_chrono(),
    }
}

/// Cash added to the drawer as a list row.
fn drawer_row(m: &CashMovementDocument, category: &ResolvedCategory) -> IncomeRow {
    let created = m.created_at.to_chrono();
    IncomeRow {
        key: m.key.clone(),
        source: IncomeSource::Drawer,
        number: m.movement_number.clone(),
        date: created.format("%Y-%m-%d").to_string(),
        category_key: m.category_key.clone(),
        category_name: category.name.clone(),
        report_group: category.report_group,
        payee_key: m.payee_key.clone(),
        payee_name: m.payee_name.clone(),
        amount_cents: m.amount_cents,
        received_into: ReceivedInto::Drawer,
        reference_no: m.reference_no.clone(),
        note: m.note.clone(),
        recorded_by_name_snapshot: m.recorded_by_name_snapshot.clone(),
        created_at: created,
    }
}

/// Every income between two days, ledger and drawer together, newest first.
///
/// shortcut: built in memory from the whole ledger plus the drawer additions of the range; a
/// shop's other income is rare, upgrade to a paged query when a shop passes tens of thousands.
async fn all_rows(db: &Db, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<IncomeRow>> {
    let incomes = repository::list_incomes(db, from, to).await?;
    let movements = sessions::service::income_movements(db, from, to).await?;
    // An addition that was reversed, and the reversal itself, cancel out. The reversal may be dated
    // outside the range asked for, so it is looked up by key, not by date.
    let movement_keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = sessions::service::reversed_keys(db, &movement_keys).await?;
    let movements: Vec<CashMovementDocument> = movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .collect();

    let mut keys: Vec<String> = incomes
        .iter()
        .map(|i| i.category_key.clone())
        .chain(movements.iter().map(|m| m.category_key.clone()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    let categories = cash_categories::service::resolve_categories(db, &keys).await?;

    let mut rows: Vec<IncomeRow> = incomes
        .iter()
        .map(|i| ledger_row(i, &categories[&i.category_key]))
        .collect();
    // Cash added to the drawer is income only under an income category (not a float, capital or a loan).
    rows.extend(
        movements
            .iter()
            .filter(|m| categories[&m.category_key].report_group == ReportGroup::OtherIncome)
            .map(|m| drawer_row(m, &categories[&m.category_key])),
    );
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.created_at.cmp(&a.created_at)));
    Ok(rows)
}

/// What was received into the bank, a card or online on or after `since` (`YYYY-MM-DD`), as `(account, cents)`.
/// Every income dated between two days (`YYYY-MM-DD`, inclusive), cash added to the drawer included, for the money reports.
pub(crate) async fn rows_between(db: &Db, from: &str, to: &str) -> AppResult<Vec<IncomeRow>> {
    all_rows(db, Some(from), Some(to)).await
}

pub(crate) async fn account_receipts_since(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(ReceivedInto, i64)>> {
    Ok(repository::list_incomes(db, Some(since), None)
        .await?
        .into_iter()
        .map(|i| (i.received_into, i.amount_cents))
        .collect())
}

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

/// Headline figures: this month's and last month's other income.
pub async fn get_stats(db: &Db) -> AppResult<IncomeStats> {
    let rows = all_rows(db, None, None).await?;
    let today = Utc::now().date_naive();
    let month = today.format("%Y-%m").to_string();
    let last_month = (today - Months::new(1)).format("%Y-%m").to_string();
    let mut stats = IncomeStats {
        this_month_cents: 0,
        this_month_drawer_cents: 0,
        last_month_cents: 0,
    };
    for r in &rows {
        if r.date.starts_with(&month) {
            stats.this_month_cents += r.amount_cents;
            if r.source == IncomeSource::Drawer {
                stats.this_month_drawer_cents += r.amount_cents;
            }
        } else if r.date.starts_with(&last_month) {
            stats.last_month_cents += r.amount_cents;
        }
    }
    Ok(stats)
}

/// One ledger income.
pub async fn get_income(db: &Db, key: &str) -> AppResult<IncomeRow> {
    let income = repository::find_income_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Income not found.", codes::INCOME_NOT_FOUND)
        })?;
    let categories = cash_categories::service::resolve_categories(
        db,
        std::slice::from_ref(&income.category_key),
    )
    .await?;
    Ok(ledger_row(&income, &categories[&income.category_key]))
}

/// Enters money received into the bank, a card or online. Cash put into the drawer is an
/// add-cash movement, not an entry here.
pub async fn create_income(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateIncomeRequest,
    device_id: Option<String>,
) -> AppResult<IncomeRow> {
    tracked("incomes.created", async move {
        if body.received_into == ReceivedInto::Drawer {
            return Err(AppError::validation(
                "Cash put into the drawer is recorded as a cash movement. It appears here by itself.",
            ));
        }
        if body.amount_cents <= 0 {
            return Err(AppError::validation("Please enter an amount greater than zero."));
        }
        parse_day("income date", &body.income_date)?;
        let category = cash_categories::service::category_for_income(db, &body.category_key).await?;

        let typed = clean("The name", body.payee_name.as_deref(), NAME_MAX)?;
        let (payee_key, payee_name) = match (&body.payee_key, typed) {
            (Some(key), _) => {
                let resolved = payees::service::resolve_payee(db, key).await?;
                (Some(key.clone()), Some(resolved.name))
            }
            (None, Some(name)) => {
                let saved = payees::service::ensure_other_payee(db, &name).await?;
                (Some(saved.key), Some(saved.name))
            }
            (None, None) => (None, None),
        };

        let reservation = sequences::service::reserve_sequence(
            db,
            "income".to_string(),
            ReserveSequenceRequest { block_size: Some(1), device_id },
        )
        .await?;
        let income_number = format!(
            "{}{:0width$}",
            reservation.prefix,
            reservation.start,
            width = reservation.padding
        );

        let now = BsonDateTime::now();
        let income = repository::insert_income(
            db,
            IncomeDocument {
                id: None,
                key: generate_id(prefixes::INCOME),
                income_number,
                income_date: body.income_date.trim().to_string(),
                category_key: category.key,
                payee_key,
                payee_name,
                amount_cents: body.amount_cents,
                received_into: body.received_into,
                reference_no: clean("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?,
                note: clean("The note", body.note.as_deref(), TEXT_MAX)?,
                recorded_by_user_id,
                recorded_by_name_snapshot: recorded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        get_income(db, &income.key).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_must_be_real_dates() {
        assert!(parse_day("date", "2026-10-23").is_ok());
        assert!(parse_day("date", "2026-02-30").is_err());
    }

    #[test]
    fn text_is_trimmed_and_limited() {
        assert_eq!(
            clean("Note", Some("  hi "), 10).unwrap().as_deref(),
            Some("hi")
        );
        assert_eq!(clean("Note", Some("   "), 10).unwrap(), None);
        assert!(clean("Note", Some("abcdefghijk"), 10).is_err());
    }
}
