use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovementType;

use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::movements;

/// Drawer payments that can be business costs (every outgoing kind but owner drawings), between
/// two days, for the expense ledger to read. The ledger keeps only those whose category counts as a cost.
pub(crate) async fn expense_movements(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<CashMovementDocument>> {
    movements::list_movements_of_types(
        db,
        &[
            CashMovementType::PettyExpense,
            CashMovementType::RentAndBills,
            CashMovementType::StaffPayment,
            CashMovementType::SupplierPayment,
        ],
        from,
        to,
    )
    .await
}
