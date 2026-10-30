use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovementType;

use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::movements;

/// Cash put into the drawer between two days, for the income ledger to read. The ledger keeps
/// only those whose category counts as income (not a float, capital or a loan).
pub(crate) async fn income_movements(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<CashMovementDocument>> {
    movements::list_movements_of_types(db, &[CashMovementType::AddCash], from, to).await
}
