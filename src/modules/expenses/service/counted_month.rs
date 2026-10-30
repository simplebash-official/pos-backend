use crate::domain::expenses::ExpenseRow;

/// The month (`YYYY-MM`) a cost counts towards: the period a drawer payment names, else its own date.
pub(crate) fn counted_month(row: &ExpenseRow) -> &str {
    &row.period_start.as_deref().unwrap_or(&row.date)[..7]
}
