use crate::modules::sessions::model::DrawerStateDocument;

/// What the drawer should hold when a shift opens: its running balance. A terminal that has never
/// held cash has nothing to compare the count with, so the count itself stands.
pub(crate) fn expected_opening_float(
    state: Option<&DrawerStateDocument>,
    counted_cents: i64,
) -> i64 {
    state.map_or(counted_cents, |s| s.current_balance_cents)
}
