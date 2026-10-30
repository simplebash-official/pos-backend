use crate::domain::cash_categories::normalize_name;
use crate::domain::sessions::CashMovementType;
use crate::seeds::cash_categories::SYSTEM_CATEGORIES;

use super::*;

#[test]
fn similar_names_catch_typos_and_containment() {
    assert!(names_are_similar("fuel", "fule"));
    assert!(names_are_similar("tea", "teaandrefreshments"));
    assert!(names_are_similar("fuel", "fuel"));
    assert!(!names_are_similar("fuel", "stationery"));
    assert!(!names_are_similar("ab", "xy"));
}

#[test]
fn names_are_cleaned_and_bounded() {
    assert_eq!(
        clean_name("  Generator   diesel ").unwrap(),
        "Generator diesel"
    );
    assert!(clean_name("a").is_err());
    assert!(clean_name(&"x".repeat(41)).is_err());
    assert_eq!(clean_name_si("   ").unwrap(), None);
}

#[test]
fn every_seeded_type_has_an_other_unless_it_is_a_drop() {
    for t in [
        CashMovementType::PettyExpense,
        CashMovementType::RentAndBills,
        CashMovementType::SupplierPayment,
        CashMovementType::OwnerWithdrawal,
        CashMovementType::AddCash,
        CashMovementType::NoSale,
    ] {
        assert!(
            SYSTEM_CATEGORIES
                .iter()
                .any(|(ty, n, _)| *ty == t && *n == OTHER_NAME),
            "{t:?} needs an Other category"
        );
    }
}

#[test]
fn seeded_names_are_unique_per_type() {
    let mut seen = std::collections::HashSet::new();
    for (t, n, _) in SYSTEM_CATEGORIES {
        assert!(
            seen.insert((t.as_str(), normalize_name(n))),
            "duplicate {n} for {t:?}"
        );
    }
}
