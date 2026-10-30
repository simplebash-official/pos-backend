use crate::domain::sessions::CashMovementType;

/// Types that move accountable money and need the managed-payout permission on top of the basic one.
#[must_use]
pub fn requires_managed_permission(movement_type: CashMovementType) -> bool {
    matches!(
        movement_type,
        CashMovementType::RentAndBills
            | CashMovementType::StaffPayment
            | CashMovementType::SupplierPayment
    )
}
