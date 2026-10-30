use super::one_movement::one_movement;
use super::prepare_movement::prepare_movement;
use super::write_movement::write_movement;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::core::logging::domain::tracked;

use crate::domain::purchase_receipts::PaymentKind;

use crate::domain::sessions::{CashMovement, CashMovementType, CreateCashMovementRequest};
use crate::modules::purchases::service::settlement;

/// Records one drawer cash event. Validates the shift, the amount, the category, the fields
/// this type needs and (where required) a manager PIN, then books it on the shift and the drawer.
pub async fn record_movement(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateCashMovementRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    tracked("cash_movement.recorded", async move {
        let requested = body.settlements.clone();
        if !requested.is_empty() && body.movement_type != CashMovementType::SupplierPayment {
            return Err(AppError::validation(
                "Receipts can only be paid by a supplier payment.",
            ));
        }
        let document = prepare_movement(
            db,
            recorded_by_user_id,
            recorded_by_name,
            body,
            device_id,
            None,
            0,
        )
        .await?;
        // The payee is a resolved supplier by now; its receipts are checked before anything is written.
        let settlements = if requested.is_empty() {
            None
        } else {
            let supplier_key = document
                .payee_key
                .as_deref()
                .ok_or_else(|| AppError::internal("A supplier payment must have a supplier"))?;
            Some(
                settlement::prepare_settlements(
                    db,
                    supplier_key,
                    document.amount_cents,
                    &requested,
                )
                .await?,
            )
        };
        let mut tx = db.begin().await?;
        let inserted = write_movement(&mut tx, document).await?;
        if let Some(prepared) = settlements {
            settlement::write_settlements(
                &mut tx,
                prepared,
                PaymentKind::CashMovement,
                &inserted.key,
            )
            .await?;
        }
        tx.commit().await?;
        one_movement(db, inserted).await
    })
    .await
}
