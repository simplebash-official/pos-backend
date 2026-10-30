use super::payout::Payout;
use super::reversed_keys::reversed_keys;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovementType;

use crate::modules::sessions::repository::movements;

/// Every drawer movement of one type, reversed ones and their reversals left out.
pub(crate) async fn payouts_of_type(
    db: &Db,
    movement_type: CashMovementType,
) -> AppResult<Vec<Payout>> {
    let movements = movements::list_movements_of_types(db, &[movement_type], None, None).await?;
    let keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = reversed_keys(db, &keys).await?;
    Ok(movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .map(|m| Payout {
            day: m.created_at.to_chrono().format("%Y-%m-%d").to_string(),
            payee_key: m.payee_key,
            category_key: m.category_key,
            number: m.movement_number,
            amount_cents: m.amount_cents,
        })
        .collect())
}
