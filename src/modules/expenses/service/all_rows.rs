use super::ledger_row::ledger_row;
use super::today::today;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::expenses::{ExpenseRow, ExpenseSource, ExpenseStatus, PaidFrom};
use crate::modules::cash_categories::service::ResolvedCategory;
use crate::modules::expenses::model::ExpensePaymentDocument;
use crate::modules::expenses::repository;
use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::{cash_categories, sessions};
use std::collections::HashMap;

/// A drawer payment as a list row. Always paid in full, from the drawer.
fn drawer_row(m: &CashMovementDocument, category: &ResolvedCategory) -> ExpenseRow {
    let created = m.created_at.to_chrono();
    ExpenseRow {
        key: m.key.clone(),
        source: ExpenseSource::Drawer,
        number: m.movement_number.clone(),
        date: created.format("%Y-%m-%d").to_string(),
        category_key: m.category_key.clone(),
        category_name: category.name.clone(),
        report_group: category.report_group,
        payee_key: m.payee_key.clone(),
        payee_name: m.payee_name.clone(),
        amount_cents: m.amount_cents,
        paid_cents: m.amount_cents,
        balance_cents: 0,
        status: ExpenseStatus::Paid,
        paid_from: PaidFrom::Drawer,
        due_date: None,
        period_start: m.period_start.clone(),
        is_overdue: false,
        reference_no: m.reference_no.clone(),
        note: m.note.clone(),
        movement_type: Some(m.movement_type),
        recorded_by_name_snapshot: m.recorded_by_name_snapshot.clone(),
        created_at: created,
    }
}

/// Every cost between two days, ledger and drawer together, newest first.
///
/// shortcut: built in memory from the whole ledger plus the drawer payments of the range; a
/// shop's costs fit comfortably, upgrade to a paged query when a shop passes tens of thousands.
pub(super) async fn all_rows(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<ExpenseRow>> {
    let expenses = repository::list_expenses(db, from, to).await?;
    let movements = sessions::service::expense_movements(db, from, to).await?;
    // A drawer payment that was reversed, and the reversal itself, cancel out and are not costs. The
    // reversal may be dated outside the range asked for, so it is looked up by key, not by date.
    let movement_keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = sessions::service::reversed_keys(db, &movement_keys).await?;
    let movements: Vec<CashMovementDocument> = movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .collect();
    let payments = repository::list_all_payments(db).await?;
    let mut by_expense: HashMap<&str, Vec<&ExpensePaymentDocument>> = HashMap::new();
    for p in &payments {
        by_expense
            .entry(p.expense_key.as_str())
            .or_default()
            .push(p);
    }

    let mut keys: Vec<String> = expenses
        .iter()
        .map(|e| e.category_key.clone())
        .chain(movements.iter().map(|m| m.category_key.clone()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    let categories = cash_categories::service::resolve_categories(db, &keys).await?;

    let day = today();
    let mut rows: Vec<ExpenseRow> = expenses
        .iter()
        .map(|e| {
            let paid = by_expense
                .get(e.key.as_str())
                .map_or(&[][..], Vec::as_slice);
            ledger_row(e, paid, &categories[&e.category_key], &day)
        })
        .collect();
    // A drawer payment is a cost only when its category counts as one (not a loan instalment).
    rows.extend(
        movements
            .iter()
            .filter(|m| {
                cash_categories::service::is_expense_group(categories[&m.category_key].report_group)
            })
            .map(|m| drawer_row(m, &categories[&m.category_key])),
    );
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.created_at.cmp(&a.created_at)));
    Ok(rows)
}
