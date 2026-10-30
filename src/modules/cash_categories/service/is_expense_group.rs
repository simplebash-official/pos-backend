use crate::domain::cash_categories::ReportGroup;

/// Report groups whose costs belong in the expense ledger.
pub(crate) fn is_expense_group(group: ReportGroup) -> bool {
    matches!(
        group,
        ReportGroup::OperatingExpense | ReportGroup::Purchases | ReportGroup::Payroll
    )
}
