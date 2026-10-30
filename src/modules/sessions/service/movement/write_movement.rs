use super::drawer_delta::drawer_delta;

use crate::clients::uow::Tx;

use crate::core::error::AppResult;

use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::{drawer, movements, sessions};

/// Books a validated movement on the shift and the drawer, in the caller's transaction, so the
/// row, the shift's totals and the drawer's cash move together or not at all.
pub(crate) async fn write_movement(
    tx: &mut Tx,
    document: CashMovementDocument,
) -> AppResult<CashMovementDocument> {
    let session_key = document.session_key.clone();
    let terminal_id = document.terminal_id.clone();
    let movement_type = document.movement_type;
    let amount_cents = document.amount_cents;

    let inserted = movements::insert_cash_movement(&mut *tx, document).await?;
    if let Some(session_key) = &session_key {
        sessions::increment_session_movements(&mut *tx, session_key, movement_type, amount_cents)
            .await?;
    }
    // The drawer balance survives across shifts, so it moves with every movement.
    let delta = drawer_delta(movement_type, amount_cents);
    if delta != 0 {
        drawer::update_drawer_balance(&mut *tx, &terminal_id, delta).await?;
    }
    Ok(inserted)
}
