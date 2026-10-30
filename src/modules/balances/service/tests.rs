use super::*;

#[test]
fn a_balance_is_the_opening_plus_increases_less_reductions() {
    assert_eq!(owed(1000, 500, 300), 1200);
    assert_eq!(owed(0, 0, 250), -250, "paid ahead is a negative balance");
}

#[test]
fn opening_keys_follow_the_subject() {
    assert_eq!(opening_key("sup_abc"), "bopen_sup_abc");
}
