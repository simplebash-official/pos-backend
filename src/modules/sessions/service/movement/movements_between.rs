use super::reversed_keys::reversed_keys;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovementType;

use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::movements;

/// Every drawer movement recorded between two days (`YYYY-MM-DD`, UTC, inclusive), reversed ones and their
/// reversals left out, for the cash flow report.
pub(crate) async fn movements_between(
    db: &Db,
    from: &str,
    to: &str,
) -> AppResult<Vec<CashMovementDocument>> {
    let all = [
        CashMovementType::PettyExpense,
        CashMovementType::RentAndBills,
        CashMovementType::StaffPayment,
        CashMovementType::SupplierPayment,
        CashMovementType::OwnerWithdrawal,
        CashMovementType::AddCash,
        CashMovementType::CashDrop,
    ];
    let movements = movements::list_movements_of_types(db, &all, Some(from), Some(to)).await?;
    let keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = reversed_keys(db, &keys).await?;
    Ok(movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .collect())
}
