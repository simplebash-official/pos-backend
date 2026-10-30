use crate::domain::incomes::{IncomeRow, IncomeSource};
use crate::modules::cash_categories::service::ResolvedCategory;
use crate::modules::incomes::model::IncomeDocument;

/// A ledger income as a list row.
pub(super) fn ledger_row(i: &IncomeDocument, category: &ResolvedCategory) -> IncomeRow {
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
