use super::to_movements::to_movements;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovement;

use crate::modules::sessions::model::CashMovementDocument;

/// One stored row as the API shape.
pub(super) async fn one_movement(db: &Db, doc: CashMovementDocument) -> AppResult<CashMovement> {
    Ok(to_movements(db, vec![doc]).await?.remove(0))
}
