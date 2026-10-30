use crate::domain::sessions::CashMovementType;

/// How a movement changes the cash in the drawer: added cash raises it, every payout and drop lowers it.
pub(super) fn drawer_delta(movement_type: CashMovementType, amount_cents: i64) -> i64 {
    use CashMovementType as T;
    match movement_type {
        T::AddCash => amount_cents,
        T::PettyExpense
        | T::RentAndBills
        | T::StaffPayment
        | T::SupplierPayment
        | T::OwnerWithdrawal
        | T::CashDrop => -amount_cents,
        T::NoSale => 0,
    }
}
