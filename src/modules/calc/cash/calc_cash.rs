use super::{CalcCashCountReply, CalcCashCountRequest};
use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::{CashMovementType, VARIANCE_TOLERANCE_CENTS};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
/// Why a cash movement cannot be made.
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

const MAX_CENTS: i64 = 10_000_000_000_000;

pub(super) fn check_cents(label: &str, cents: i64) -> AppResult<()> {
    if !(0..=MAX_CENTS).contains(&cents) {
        return Err(AppError::validation(format!("{label} is out of range")));
    }
    Ok(())
}

/// Computes the count.
pub fn compute_count(request: &CalcCashCountRequest) -> AppResult<CalcCashCountReply> {
    Ok(pos_calc_core::cash::compute_count(request)?)
}

const MAX_SETTLEMENTS: usize = 200;

/// Computes the movement.
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

/// What the handler reads for the request.
#[derive(Debug)]
pub struct MovementContext {
    pub cash_in_drawer_cents: i64,
    pub category_limit_cents: Option<i64>,
    pub shop_limit_cents: Option<i64>,
    pub period_already_paid: bool,
}
