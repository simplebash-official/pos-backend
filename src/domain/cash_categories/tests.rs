use super::domain_cash_categories::category_key;
use super::domain_cash_categories::normalize_name;
use crate::domain::sessions::CashMovementType;

#[test]
fn names_normalize_without_case_or_spacing() {
    assert_eq!(
        normalize_name("  Tea and  Refreshments "),
        "teaandrefreshments"
    );
}

#[test]
fn category_key_is_stable_and_per_type() {
    let a = category_key(CashMovementType::PettyExpense, "Fuel");
    assert_eq!(a, category_key(CashMovementType::PettyExpense, " fuel "));
    assert_ne!(a, category_key(CashMovementType::RentAndBills, "Fuel"));
    assert!(a.starts_with("ccat_petty_expense_"));
}
