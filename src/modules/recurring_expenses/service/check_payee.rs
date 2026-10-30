use crate::core::error::{AppError, AppResult};
use crate::domain::expenses::PaidFrom;
use crate::domain::sessions::CashMovementType;

/// Rent and bills paid from the drawer are always paid to someone (the cash form asks who), so a
/// regular bill that will be paid that way must name them or it could never be paid.
pub(super) fn check_payee(
    movement_type: CashMovementType,
    paid_from: PaidFrom,
    payee_key: Option<&str>,
) -> AppResult<()> {
    if movement_type == CashMovementType::RentAndBills
        && paid_from == PaidFrom::Drawer
        && payee_key.is_none()
    {
        return Err(AppError::validation("Pick who is being paid."));
    }
    Ok(())
}
