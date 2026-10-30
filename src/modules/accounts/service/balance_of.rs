use crate::domain::accounts::AccountActivity;

/// What an account holds: where it started, plus what came in, less what went out.
pub(super) fn balance_of(opening_cents: i64, a: &AccountActivity) -> i64 {
    opening_cents + a.sales_cents - a.refunds_cents + a.income_cents + a.deposits_cents
        - a.expenses_cents
        - a.withdrawals_cents
}
