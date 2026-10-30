use super::calc_cash_movement_reply::CalcCashMovementReply;
use super::calc_cash_movement_request::CalcCashMovementRequest;
use super::cash_movement_blocker::CashMovementBlocker;
use super::check_cents::check_cents;
use super::movement_context::MovementContext;
use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::CashMovementType;

const MAX_SETTLEMENTS: usize = 200;

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
