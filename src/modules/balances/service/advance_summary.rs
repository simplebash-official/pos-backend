use super::given::Given;
use super::owed::owed;
use crate::domain::balances::StaffAdvance;
use crate::modules::balances::model::{AdvanceRecoveryDocument, BalanceOpeningDocument};

/// What one staff member owes: the opening (if any), plus advances given, less advances recovered, both from the opening day.
pub(super) fn advance_summary(
    employee_key: &str,
    name: String,
    opening: Option<&BalanceOpeningDocument>,
    given: &[&Given],
    recoveries: &[&AdvanceRecoveryDocument],
) -> StaffAdvance {
    let since = opening.map(|o| o.as_of_date.as_str());
    let counts = |day: &str| since.is_none_or(|s| day >= s);
    let given_cents: i64 = given
        .iter()
        .filter(|g| counts(&g.day))
        .map(|g| g.cents)
        .sum();
    let recovered_cents: i64 = recoveries
        .iter()
        .filter(|r| counts(&r.recovered_on))
        .map(|r| r.amount_cents)
        .sum();
    StaffAdvance {
        employee_key: employee_key.to_string(),
        name,
        opening_balance_cents: opening.map(|o| o.opening_balance_cents),
        as_of_date: opening.map(|o| o.as_of_date.clone()),
        given_cents,
        recovered_cents,
        outstanding_cents: owed(
            opening.map_or(0, |o| o.opening_balance_cents),
            given_cents,
            recovered_cents,
        ),
        last_given_on: given.iter().map(|g| g.day.clone()).max(),
    }
}
