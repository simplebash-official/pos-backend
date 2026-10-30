use super::reversed_keys::reversed_keys;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovementType;

use crate::modules::sessions::repository::movements;

/// Cash taken to the bank or brought back from it, from `since` (`YYYY-MM-DD`) on, as
/// `(deposited, withdrawn)`. A deposit is a cash drop under any of `deposit_categories`, a withdrawal an
/// add-cash under any of `withdrawal_categories`; reversed movements and their reversals cancel out.
pub(crate) async fn bank_cash_moves(
    db: &Db,
    since: &str,
    deposit_categories: &[String],
    withdrawal_categories: &[String],
) -> AppResult<(i64, i64)> {
    let movements = movements::list_movements_of_types(
        db,
        &[CashMovementType::CashDrop, CashMovementType::AddCash],
        Some(since),
        None,
    )
    .await?;
    let keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = reversed_keys(db, &keys).await?;
    let (mut deposited, mut withdrawn) = (0, 0);
    for m in movements
        .iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
    {
        match m.movement_type {
            CashMovementType::CashDrop if deposit_categories.contains(&m.category_key) => {
                deposited += m.amount_cents
            }
            CashMovementType::AddCash if withdrawal_categories.contains(&m.category_key) => {
                withdrawn += m.amount_cents;
            }
            _ => {}
        }
    }
    Ok((deposited, withdrawn))
}
