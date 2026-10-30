use crate::domain::cash_settings::UpdateCashSettingsRequest;

use super::*;

fn ok() -> UpdateCashSettingsRequest {
    UpdateCashSettingsRequest {
        undo_window_seconds: 120,
        approval_threshold_cents: Some(50_000),
        quick_amounts_cents: vec![10_000, 50_000],
        float_target_cents: None,
    }
}

#[test]
fn a_good_set_of_rules_passes() {
    assert!(validate(&ok()).is_ok());
}

#[test]
fn each_rule_is_checked() {
    let bad = |f: fn(&mut UpdateCashSettingsRequest)| {
        let mut b = ok();
        f(&mut b);
        validate(&b).is_err()
    };
    assert!(bad(|b| b.undo_window_seconds = 5));
    assert!(bad(|b| b.undo_window_seconds = 4000));
    assert!(bad(|b| b.approval_threshold_cents = Some(0)));
    assert!(bad(|b| b.float_target_cents = Some(-1)));
    assert!(bad(|b| b.quick_amounts_cents = vec![]));
    assert!(bad(|b| b.quick_amounts_cents = vec![5_000, 5_000]));
    assert!(bad(|b| b.quick_amounts_cents = vec![50_000, 10_000]));
    assert!(bad(
        |b| b.quick_amounts_cents = (1..=9).map(|n| n * 1000).collect()
    ));
}
