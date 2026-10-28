// The cash screens' live figures: what a drawer payment does to the drawer and what stops it being
// saved, what a counted pile of notes and coins comes to, and how a shift's count compares with
// what the drawer should hold. The rules are `domain::sessions` (the same code the movement and
// shift saves use); the handler reads the drawer, the category and the shop limit, this module
// only combines them.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    core::error::{AppError, AppResult},
    domain::sessions::{CashMovementType, VARIANCE_TOLERANCE_CENTS},
};

const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_SETTLEMENTS: usize = 200;

fn check_cents(label: &str, cents: i64) -> AppResult<()> {
    if !(0..=MAX_CENTS).contains(&cents) {
        return Err(AppError::validation(format!("{label} is out of range")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Counting notes and coins
// ---------------------------------------------------------------------------

pub use pos_calc_core::cash::{CalcCashCountReply, CalcCashCountRequest, CashCountLine};

pub fn compute_count(request: &CalcCashCountRequest) -> AppResult<CalcCashCountReply> {
    Ok(pos_calc_core::cash::compute_count(request)?)
}

// ---------------------------------------------------------------------------
// A shift's count against the drawer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftCountRequest {
    /// Counting at close: the open shift.
    pub session_key: Option<String>,
    /// Counting at open: the terminal whose drawer is being counted.
    pub terminal_id: Option<String>,
    pub counted_cents: i64,
    /// At close, the float left in the drawer for the next shift. Absent until the person chooses
    /// one; the reply then suggests keeping the opening float.
    pub closing_float_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftCountReply {
    /// The count that was asked about.
    pub counted_cents: i64,
    /// What the drawer should hold.
    pub expected_cents: i64,
    /// Counted less expected; negative is a shortage.
    pub variance_cents: i64,
    /// Any difference needs an explanation.
    pub reason_required: bool,
    /// A manager PIN is needed: any difference at open, one beyond the tolerance at close.
    pub pin_required: bool,
    /// How far off a closing count can be before it needs a manager PIN.
    pub variance_tolerance_cents: i64,
    /// At close: the float that stays in the drawer, the one chosen or else the opening float.
    pub closing_float_cents: Option<i64>,
    /// At close: the opening float, or the whole count when that is less (the "keep opening float" button).
    pub keep_opening_float_cents: Option<i64>,
    /// At close: the cash taken to the safe, counted less the float left.
    pub safe_deposit_cents: Option<i64>,
}

/// `opening_float_cents` is the shift's opening float, known only when closing.
pub fn compute_shift_count(
    request: &CalcShiftCountRequest,
    expected_cents: i64,
    opening_float_cents: Option<i64>,
) -> AppResult<CalcShiftCountReply> {
    check_cents("The count", request.counted_cents)?;
    let counted = request.counted_cents;
    let variance_cents = counted - expected_cents;
    let (closing_float_cents, keep_opening_float_cents, safe_deposit_cents) =
        match (opening_float_cents, request.closing_float_cents) {
            (Some(opening), chosen) => {
                let keep = counted.min(opening);
                let float = chosen.unwrap_or(keep);
                check_cents("The float", float)?;
                if float > counted {
                    return Err(AppError::validation(
                        "The float cannot be more than the count",
                    ));
                }
                (Some(float), Some(keep), Some(counted - float))
            }
            (None, Some(_)) => {
                return Err(AppError::validation("A float is only left when closing"));
            }
            (None, None) => (None, None, None),
        };
    Ok(CalcShiftCountReply {
        counted_cents: counted,
        expected_cents,
        variance_cents,
        reason_required: variance_cents != 0,
        pin_required: if opening_float_cents.is_some() {
            variance_cents.abs() > VARIANCE_TOLERANCE_CENTS
        } else {
            variance_cents != 0
        },
        variance_tolerance_cents: VARIANCE_TOLERANCE_CENTS,
        closing_float_cents,
        keep_opening_float_cents,
        safe_deposit_cents,
    })
}

// ---------------------------------------------------------------------------
// A drawer payment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCashMovementRequest {
    pub movement_type: CashMovementType,
    pub amount_cents: i64,
    pub category_key: Option<String>,
    /// The open shift; absent for cash taken out between shifts.
    pub session_key: Option<String>,
    pub terminal_id: String,
    /// A saved payee (supplier, staff member, landlord...).
    pub payee_key: Option<String>,
    /// A name typed instead of picking a saved payee.
    pub payee_name: String,
    /// `YYYY-MM`, the month a rent, bill or salary covers.
    pub period: Option<String>,
    /// What a supplier payment puts against each bill.
    pub settlement_amounts_cents: Vec<i64>,
    pub pin_entered: bool,
    pub pin_verified: bool,
    pub override_reason_entered: bool,
    pub confirm_duplicate: bool,
}

/// What the handler reads for the request.
pub struct MovementContext {
    pub cash_in_drawer_cents: i64,
    pub category_limit_cents: Option<i64>,
    pub shop_limit_cents: Option<i64>,
    pub period_already_paid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementBlocker {
    AmountMissing,
    CategoryMissing,
    /// A no-sale open: why the drawer is being opened.
    OpenReasonMissing,
    RentPayeeMissing,
    PeriodMissing,
    StaffMissing,
    SupplierMissing,
    OverAllocated,
    WhoTookCashMissing,
    PinMissing,
    PinNotVerified,
    OverrideReasonMissing,
    PeriodAlreadyPaid,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCashMovementReply {
    pub takes_cash_out: bool,
    pub cash_in_drawer_cents: i64,
    /// The drawer after this payment.
    pub cash_after_cents: i64,
    /// The change to the drawer: positive adds cash, negative takes it out.
    pub drawer_delta_cents: i64,
    pub over_drawer: bool,
    pub pin_required: bool,
    /// The month a rent or bill covers already has a payment.
    pub period_already_paid: bool,
    /// What stops the payment being saved, in the order to say it; empty means it can go ahead.
    pub blockers: Vec<CashMovementBlocker>,
}

pub fn compute_movement(
    request: &CalcCashMovementRequest,
    context: &MovementContext,
) -> AppResult<CalcCashMovementReply> {
    check_cents("The amount", request.amount_cents)?;
    if request.settlement_amounts_cents.len() > MAX_SETTLEMENTS {
        return Err(AppError::validation("Too many bills"));
    }
    let mut allocated: i64 = 0;
    for cents in &request.settlement_amounts_cents {
        check_cents("A bill amount", *cents)?;
        allocated = allocated
            .checked_add(*cents)
            .ok_or_else(|| AppError::validation("That amount is too large"))?;
    }

    use CashMovementBlocker as B;
    use CashMovementType as T;
    let kind = request.movement_type;
    let amount = request.amount_cents;
    let over_drawer = kind.is_over_drawer(amount, context.cash_in_drawer_cents);
    let pin_required = kind.needs_manager_pin(
        amount,
        context.category_limit_cents,
        context.shop_limit_cents,
        over_drawer,
    );
    let rent_paid = kind == T::RentAndBills && context.period_already_paid;

    let mut blockers = Vec::new();
    if kind != T::NoSale && amount == 0 {
        blockers.push(B::AmountMissing);
    }
    if request.category_key.is_none() {
        blockers.push(if kind == T::NoSale {
            B::OpenReasonMissing
        } else {
            B::CategoryMissing
        });
    }
    match kind {
        T::RentAndBills => {
            if request.payee_key.is_none() {
                blockers.push(B::RentPayeeMissing);
            }
            if request.period.is_none() {
                blockers.push(B::PeriodMissing);
            }
        }
        T::StaffPayment => {
            if request.payee_key.is_none() {
                blockers.push(B::StaffMissing);
            }
        }
        T::SupplierPayment => {
            if request.payee_key.is_none() {
                blockers.push(B::SupplierMissing);
            }
            if allocated > amount {
                blockers.push(B::OverAllocated);
            }
        }
        T::OwnerWithdrawal if request.payee_name.trim().is_empty() => {
            blockers.push(B::WhoTookCashMissing);
        }
        _ => {}
    }
    if pin_required {
        if !request.pin_entered {
            blockers.push(B::PinMissing);
        } else if !request.pin_verified {
            blockers.push(B::PinNotVerified);
        }
    }
    if over_drawer && !request.override_reason_entered {
        blockers.push(B::OverrideReasonMissing);
    }
    if rent_paid && !request.confirm_duplicate {
        blockers.push(B::PeriodAlreadyPaid);
    }

    let drawer_delta_cents = kind.drawer_delta_cents(amount);
    Ok(CalcCashMovementReply {
        takes_cash_out: kind.takes_cash_out(),
        cash_in_drawer_cents: context.cash_in_drawer_cents,
        cash_after_cents: context.cash_in_drawer_cents + drawer_delta_cents,
        drawer_delta_cents,
        over_drawer,
        pin_required,
        period_already_paid: rent_paid,
        blockers,
    })
}

#[cfg(test)]
mod tests {
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
        let open =
            compute_movement(&movement(CashMovementType::NoSale, 0), &context(100_000)).unwrap();
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
        let exact =
            compute_shift_count(&close(100_000, Some(30_000)), 100_000, Some(200_000)).unwrap();
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
}
