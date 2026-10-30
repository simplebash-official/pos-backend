use super::ledger_row::ledger_row;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_categories::ReportGroup;
use crate::domain::incomes::{IncomeRow, IncomeSource, ReceivedInto};
use crate::modules::cash_categories::service::ResolvedCategory;
use crate::modules::incomes::repository;
use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::{cash_categories, sessions};

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
pub(super) async fn all_rows(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<IncomeRow>> {
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
