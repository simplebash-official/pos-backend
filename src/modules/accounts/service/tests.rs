use crate::domain::accounts::{AccountActivity, AccountKind};

use super::*;

#[test]
fn a_balance_is_the_opening_plus_in_less_out() {
    let a = AccountActivity {
        sales_cents: 1000,
        refunds_cents: 100,
        income_cents: 50,
        expenses_cents: 300,
        deposits_cents: 700,
        withdrawals_cents: 200,
    };
    assert_eq!(
        balance_of(5000, &a),
        5000 + 1000 - 100 + 50 + 700 - 300 - 200
    );
    assert_eq!(balance_of(-250, &AccountActivity::default()), -250);
}

#[test]
fn keys_are_fixed_per_account() {
    assert_eq!(opening_key(AccountKind::Bank), "aopen_bank");
}
