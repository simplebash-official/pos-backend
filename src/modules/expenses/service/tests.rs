use crate::domain::expenses::ExpenseStatus;

use super::*;

#[test]
fn status_follows_what_is_paid() {
    assert_eq!(ExpenseStatus::of(1000, 0), ExpenseStatus::Unpaid);
    assert_eq!(ExpenseStatus::of(1000, 400), ExpenseStatus::PartPaid);
    assert_eq!(ExpenseStatus::of(1000, 1000), ExpenseStatus::Paid);
}

#[test]
fn days_must_be_real_dates() {
    assert!(parse_day("date", "2026-10-23").is_ok());
    assert!(parse_day("date", "2026-02-30").is_err());
    assert!(parse_day("date", "23/10/2026").is_err());
}
