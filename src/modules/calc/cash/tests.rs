use super::calc_cash_movement_request::CalcCashMovementRequest;
use super::calc_shift_count_request::CalcShiftCountRequest;
use super::cash_movement_blocker::CashMovementBlocker;
use super::compute_count::compute_count;
use super::compute_movement::compute_movement;
use super::compute_shift_count::compute_shift_count;
use super::movement_context::MovementContext;

use crate::domain::sessions::CashMovementType;

use super::*;
use std::collections::HashMap;

fn movement(kind: CashMovementType, amount: i64) -> CalcCashMovementRequest {
    CalcCashMovementRequest {
        movement_type: kind,
        amount_cents: amount,
        category_key: Some("ccat_x".into()),
        session_key: None,
        terminal_id: "t1".into(),
        payee_key: Some("pye_x".into()),
        payee_name: String::new(),
        period: Some("2026-10".into()),
        settlement_amounts_cents: vec![],
        pin_entered: false,
        pin_verified: false,
        override_reason_entered: false,
        confirm_duplicate: false,
    }
}

fn context(cash: i64) -> MovementContext {
    MovementContext {
        cash_in_drawer_cents: cash,
        category_limit_cents: None,
        shop_limit_cents: None,
        period_already_paid: false,
    }
}

#[test]
fn a_payout_lowers_the_drawer_and_added_cash_raises_it() {
    let out = compute_movement(
        &movement(CashMovementType::PettyExpense, 30_000),
        &context(100_000),
    )
    .unwrap();
    assert_eq!(
        (out.drawer_delta_cents, out.cash_after_cents),
        (-30_000, 70_000)
    );
    assert!(out.blockers.is_empty());
    let add = compute_movement(
        &movement(CashMovementType::AddCash, 30_000),
        &context(100_000),
    )
    .unwrap();
    assert_eq!(add.cash_after_cents, 130_000);
    let open = compute_movement(&movement(CashMovementType::NoSale, 0), &context(100_000)).unwrap();
    assert_eq!(open.drawer_delta_cents, 0);
    assert!(open.blockers.is_empty());
}

#[test]
fn going_over_the_drawer_needs_a_pin_and_a_reason() {
    let reply = compute_movement(
        &movement(CashMovementType::PettyExpense, 150_000),
        &context(100_000),
    )
    .unwrap();
    assert!(reply.over_drawer && reply.pin_required);
    assert_eq!(
        reply.blockers,
        vec![
            CashMovementBlocker::PinMissing,
            CashMovementBlocker::OverrideReasonMissing
        ]
    );
    let mut request = movement(CashMovementType::PettyExpense, 150_000);
    request.pin_entered = true;
    let reply = compute_movement(&request, &context(100_000)).unwrap();
    assert_eq!(
        reply.blockers,
        vec![
            CashMovementBlocker::PinNotVerified,
            CashMovementBlocker::OverrideReasonMissing
        ]
    );
    request.pin_verified = true;
    request.override_reason_entered = true;
    assert!(
        compute_movement(&request, &context(100_000))
            .unwrap()
            .blockers
            .is_empty()
    );
}

#[test]
fn the_shop_limit_counts_for_cash_out_only() {
    let mut ctx = context(1_000_000);
    ctx.shop_limit_cents = Some(50_000);
    assert!(
        compute_movement(&movement(CashMovementType::PettyExpense, 60_000), &ctx)
            .unwrap()
            .pin_required
    );
    assert!(
        !compute_movement(&movement(CashMovementType::AddCash, 60_000), &ctx)
            .unwrap()
            .pin_required
    );
    ctx.category_limit_cents = Some(10_000);
    assert!(
        compute_movement(&movement(CashMovementType::AddCash, 20_000), &ctx)
            .unwrap()
            .pin_required
    );
}

#[test]
fn owner_and_staff_payments_always_need_a_pin() {
    for kind in [
        CashMovementType::OwnerWithdrawal,
        CashMovementType::StaffPayment,
    ] {
        assert!(
            compute_movement(&movement(kind, 100), &context(10_000))
                .unwrap()
                .pin_required
        );
    }
}

#[test]
fn each_kind_names_what_is_missing() {
    let mut request = movement(CashMovementType::SupplierPayment, 10_000);
    request.payee_key = None;
    request.category_key = None;
    request.settlement_amounts_cents = vec![6_000, 6_000];
    let reply = compute_movement(&request, &context(1_000_000)).unwrap();
    assert_eq!(
        reply.blockers,
        vec![
            CashMovementBlocker::CategoryMissing,
            CashMovementBlocker::SupplierMissing,
            CashMovementBlocker::OverAllocated
        ]
    );
    let mut owner = movement(CashMovementType::OwnerWithdrawal, 100);
    owner.payee_name = "  ".into();
    owner.pin_entered = true;
    owner.pin_verified = true;
    assert_eq!(
        compute_movement(&owner, &context(10_000)).unwrap().blockers,
        vec![CashMovementBlocker::WhoTookCashMissing]
    );
}

#[test]
fn a_month_already_paid_needs_confirming() {
    let mut ctx = context(1_000_000);
    ctx.period_already_paid = true;
    let mut request = movement(CashMovementType::RentAndBills, 100);
    assert_eq!(
        compute_movement(&request, &ctx).unwrap().blockers,
        vec![CashMovementBlocker::PeriodAlreadyPaid]
    );
    request.confirm_duplicate = true;
    assert!(
        compute_movement(&request, &ctx)
            .unwrap()
            .blockers
            .is_empty()
    );
}

#[test]
fn notes_and_coins_add_up() {
    let counts = HashMap::from([
        ("1000".to_string(), 3),
        ("5".to_string(), 2),
        ("5000".to_string(), 1),
    ]);
    let reply = compute_count(&CalcCashCountRequest { counts }).unwrap();
    assert_eq!(reply.total_cents, 801_000);
    assert_eq!(reply.lines.len(), 10);
    assert_eq!(reply.lines[1].subtotal_cents, 300_000);
    assert!(
        compute_count(&CalcCashCountRequest {
            counts: HashMap::from([("7".to_string(), 1)])
        })
        .is_err()
    );
    assert!(
        compute_count(&CalcCashCountRequest {
            counts: HashMap::from([("5".to_string(), -1)])
        })
        .is_err()
    );
}

#[test]
fn a_shift_count_is_compared_with_the_drawer() {
    let close = |counted, float| CalcShiftCountRequest {
        session_key: Some("ses_x".into()),
        terminal_id: None,
        counted_cents: counted,
        closing_float_cents: float,
    };
    let exact = compute_shift_count(&close(100_000, Some(30_000)), 100_000, Some(200_000)).unwrap();
    assert_eq!(
        (
            exact.variance_cents,
            exact.reason_required,
            exact.pin_required
        ),
        (0, false, false)
    );
    assert_eq!(exact.safe_deposit_cents, Some(70_000));
    assert_eq!(exact.keep_opening_float_cents, Some(100_000));
    let small = compute_shift_count(&close(99_600, None), 100_000, Some(50_000)).unwrap();
    assert_eq!(
        (
            small.variance_cents,
            small.reason_required,
            small.pin_required
        ),
        (-400, true, false)
    );
    // No float chosen yet: the opening float is suggested, and the rest goes to the safe.
    assert_eq!(
        (small.closing_float_cents, small.safe_deposit_cents),
        (Some(50_000), Some(49_600))
    );
    let large = compute_shift_count(&close(99_000, None), 100_000, Some(50_000)).unwrap();
    assert!(large.pin_required);
    let open = CalcShiftCountRequest {
        session_key: None,
        terminal_id: Some("t1".into()),
        counted_cents: 99_900,
        closing_float_cents: None,
    };
    let opened = compute_shift_count(&open, 100_000, None).unwrap();
    assert!(opened.pin_required && opened.safe_deposit_cents.is_none());
    assert!(compute_shift_count(&close(1_000, Some(2_000)), 0, Some(5_000)).is_err());
    assert!(compute_shift_count(&open_with_float(), 0, None).is_err());
}

fn open_with_float() -> CalcShiftCountRequest {
    CalcShiftCountRequest {
        session_key: None,
        terminal_id: Some("t1".into()),
        counted_cents: 1,
        closing_float_cents: Some(1),
    }
}
