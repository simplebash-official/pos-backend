use crate::domain::expenses::PaidFrom;
use crate::domain::recurring_expenses::BillStatus;
use crate::domain::sessions::CashMovementType;
use chrono::NaiveDate;

use super::*;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn due_day_falls_back_to_the_last_day_of_a_short_month() {
    assert_eq!(due_date_in(day(2026, 2, 1), 31), day(2026, 2, 28));
    assert_eq!(due_date_in(day(2028, 2, 1), 30), day(2028, 2, 29));
    assert_eq!(due_date_in(day(2026, 12, 1), 31), day(2026, 12, 31));
    assert_eq!(due_date_in(day(2026, 10, 1), 1), day(2026, 10, 1));
}

#[test]
fn a_bill_moves_from_upcoming_to_due_to_overdue_and_paid_wins() {
    let due = day(2026, 10, 10);
    let s = |today, paid| status_for(today, due, 3, 1000, paid);
    assert_eq!(s(day(2026, 10, 1), 0), BillStatus::Upcoming);
    assert_eq!(s(day(2026, 10, 7), 0), BillStatus::Due);
    assert_eq!(s(day(2026, 10, 10), 0), BillStatus::Due);
    assert_eq!(s(day(2026, 10, 11), 0), BillStatus::Overdue);
    assert_eq!(s(day(2026, 10, 11), 400), BillStatus::Overdue);
    assert_eq!(s(day(2026, 10, 11), 1000), BillStatus::Paid);
}

#[test]
fn schedules_are_checked() {
    assert!(check_schedule(100, 1, 0, PaidFrom::Bank).is_ok());
    assert!(check_schedule(0, 1, 0, PaidFrom::Bank).is_err());
    assert!(check_schedule(100, 0, 0, PaidFrom::Bank).is_err());
    assert!(check_schedule(100, 32, 0, PaidFrom::Bank).is_err());
    assert!(check_schedule(100, 5, 31, PaidFrom::Bank).is_err());
    assert!(check_schedule(100, 5, 3, PaidFrom::Unpaid).is_err());
}

#[test]
fn drawer_rent_needs_a_payee_but_other_bills_do_not() {
    use CashMovementType::{PettyExpense, RentAndBills};
    assert!(check_payee(RentAndBills, PaidFrom::Drawer, None).is_err());
    assert!(check_payee(RentAndBills, PaidFrom::Drawer, Some("pye_x")).is_ok());
    assert!(check_payee(RentAndBills, PaidFrom::Bank, None).is_ok());
    assert!(check_payee(PettyExpense, PaidFrom::Drawer, None).is_ok());
}

#[test]
fn months_step_back_across_a_year() {
    assert_eq!(previous_month(day(2026, 1, 1)), day(2025, 12, 1));
    assert_eq!(month_string(day(2026, 10, 1)), "2026-10");
    assert!(parse_month("2026-13").is_err());
}
