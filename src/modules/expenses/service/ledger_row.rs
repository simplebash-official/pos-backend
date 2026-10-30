use crate::domain::expenses::{ExpenseRow, ExpenseSource, ExpenseStatus, PaidFrom};
use crate::modules::cash_categories::service::ResolvedCategory;
use crate::modules::expenses::model::{ExpenseDocument, ExpensePaymentDocument};

/// A ledger expense as a list row, with what its payments add up to.
pub(super) fn ledger_row(
    e: &ExpenseDocument,
    payments: &[&ExpensePaymentDocument],
    category: &ResolvedCategory,
    today: &str,
) -> ExpenseRow {
    let paid_cents: i64 = payments.iter().map(|p| p.amount_cents).sum();
    let status = ExpenseStatus::of(e.amount_cents, paid_cents);
    ExpenseRow {
        key: e.key.clone(),
        source: ExpenseSource::Ledger,
        number: e.expense_number.clone(),
        date: e.expense_date.clone(),
        category_key: e.category_key.clone(),
        category_name: category.name.clone(),
        report_group: category.report_group,
        payee_key: e.payee_key.clone(),
        payee_name: e.payee_name.clone(),
        amount_cents: e.amount_cents,
        paid_cents,
        balance_cents: e.amount_cents - paid_cents,
        status,
        paid_from: payments.last().map_or(PaidFrom::Unpaid, |p| p.paid_from),
        due_date: e.due_date.clone(),
        period_start: None,
        is_overdue: status != ExpenseStatus::Paid
            && e.due_date.as_deref().is_some_and(|due| due < today),
        reference_no: e.reference_no.clone(),
        note: e.note.clone(),
        movement_type: None,
        recorded_by_name_snapshot: e.recorded_by_name_snapshot.clone(),
        created_at: e.created_at.to_chrono(),
    }
}
