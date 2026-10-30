use super::clean_text::clean_text;
use super::drawer_delta::drawer_delta;
use super::note_max::NOTE_MAX;
use super::one_movement::one_movement;
use crate::clients::db::Db;
use crate::clients::uow::Tx;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::core::logging::domain::tracked;

use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::sessions::{
    CashDirection, CashMovement, CashMovementType, ReverseCashMovementRequest, SessionStatus,
};

use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::{drawer, movements, sessions};
use crate::modules::{cash_settings, sequences};

use mongodb::bson::DateTime as BsonDateTime;

/// Cancels a movement by recording its opposite; the original row is never edited or deleted. The person
/// who recorded it can undo it inside the shop's undo window (`cash_settings`); after that only someone allowed to reverse
/// (`can_reverse`) can, and must say why. Reversing twice is refused: the reversal's key is derived from
/// the original's, so two devices undoing the same movement offline are the same row.
pub async fn reverse_movement(
    db: &Db,
    key: &str,
    user_id: String,
    user_name: String,
    can_reverse: bool,
    body: ReverseCashMovementRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    tracked("cash_movement.reversed", async move {
        let original = movements::find_cash_movement_by_key(db, key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code(
                    "Cash movement not found",
                    codes::CASH_MOVEMENT_NOT_FOUND,
                )
            })?;
        if original.reverses_key.is_some() || original.movement_type == CashMovementType::NoSale {
            return Err(AppError::validation_with_code(
                "This entry cannot be reversed.",
                codes::MOVEMENT_NOT_REVERSIBLE,
            ));
        }

        // A payment made with a supplier receipt is undone together with what was recorded with
        // it (the owner's own money put in first), so the drawer is never left half-undone.
        let mut members = vec![original.clone()];
        if let Some(receipt_key) = &original.receipt_key {
            for sibling in movements::list_movements_by_receipt_key(db, receipt_key).await? {
                if sibling.key != original.key && sibling.reverses_key.is_none() {
                    members.push(sibling);
                }
            }
        }

        let mut reversals = Vec::with_capacity(members.len());
        let mut original_reversal_key = None;
        for member in &members {
            let is_original = member.key == original.key;
            if !is_original
                && movements::find_cash_movement_by_key(db, &format!("{}_rev", member.key))
                    .await?
                    .is_some()
            {
                continue; // that part was already undone on its own
            }
            let reversal = prepare_reversal(
                db,
                member,
                user_id.clone(),
                user_name.clone(),
                can_reverse,
                body.reason.as_deref(),
                device_id.clone(),
            )
            .await?;
            if is_original {
                original_reversal_key = Some(reversal.key.clone());
            }
            reversals.push(reversal);
        }

        let mut tx = db.begin().await?;
        let mut requested = None;
        for reversal in reversals {
            let is_requested = original_reversal_key.as_deref() == Some(reversal.key.as_str());
            let inserted = write_reversal(&mut tx, reversal).await?;
            if is_requested {
                requested = Some(inserted);
            }
        }
        tx.commit().await?;
        let requested = requested
            .ok_or_else(|| AppError::internal("The reversal that was asked for was not written"))?;
        one_movement(db, requested).await
    })
    .await
}

/// Checks that one movement can be reversed by this person now and builds its opposite.
async fn prepare_reversal(
    db: &Db,
    original: &CashMovementDocument,
    user_id: String,
    user_name: String,
    can_reverse: bool,
    reason: Option<&str>,
    device_id: Option<String>,
) -> AppResult<CashMovementDocument> {
    let reversal_key = format!("{}_rev", original.key);
    if movements::find_cash_movement_by_key(db, &reversal_key)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            codes::MOVEMENT_ALREADY_REVERSED,
            "This movement was already reversed.",
        ));
    }
    if let Some(session_key) = &original.session_key {
        let session = sessions::find_session_by_key(db, session_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code(
                    "Register session not found",
                    codes::SESSION_NOT_FOUND,
                )
            })?;
        if !matches!(
            session.status,
            SessionStatus::Open | SessionStatus::Reopened
        ) {
            return Err(AppError::validation_with_code(
                "That shift is closed. Reopen it first to reverse this movement.",
                codes::NO_ACTIVE_SESSION,
            ));
        }
    }

    let age = chrono::Utc::now() - original.created_at.to_chrono();
    let own_undo = original.recorded_by_user_id == user_id
        && age.num_seconds() <= cash_settings::service::undo_window_seconds(db).await?;
    let reason = clean_text("The reason", reason, NOTE_MAX)?;
    if !own_undo {
        if !can_reverse {
            return Err(AppError::forbidden_with_code(
                "Only a manager can reverse this now.",
                codes::PERMISSION_DENIED,
            ));
        }
        if reason.is_none() {
            return Err(AppError::validation(
                "Please say why this is being reversed.",
            ));
        }
    }

    let reservation = sequences::service::reserve_sequence(
        db,
        "cash_movement".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id,
        },
    )
    .await?;
    let movement_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );
    let now = BsonDateTime::now();
    Ok(CashMovementDocument {
        id: None,
        key: reversal_key,
        movement_number,
        movement_type: original.movement_type,
        // The opposite way round, so totals by direction cancel out.
        direction: match original.direction {
            CashDirection::Out => CashDirection::In,
            CashDirection::In => CashDirection::Out,
            CashDirection::Neutral => CashDirection::Neutral,
        },
        amount_cents: original.amount_cents,
        session_key: original.session_key.clone(),
        terminal_id: original.terminal_id.clone(),
        category_key: original.category_key.clone(),
        custom_label: original.custom_label.clone(),
        payee_key: original.payee_key.clone(),
        payee_name: original.payee_name.clone(),
        period_start: None,
        period_end: None,
        reference_no: None,
        note: Some(format!("Reversal of {}", original.movement_number)),
        recorded_by_user_id: user_id,
        recorded_by_name_snapshot: user_name,
        approved_by_user_id: None,
        approved_by_name_snapshot: None,
        approval_reason: reason,
        reverses_key: Some(original.key.clone()),
        recorded_late: false,
        original_variance_session_key: None,
        receipt_key: None,
        version: 1,
        created_at: now,
        updated_at: now,
    })
}

/// Writes a reversal and takes the amount back out of the shift and the drawer.
async fn write_reversal(
    tx: &mut Tx,
    reversal: CashMovementDocument,
) -> AppResult<CashMovementDocument> {
    let session_key = reversal.session_key.clone();
    let terminal_id = reversal.terminal_id.clone();
    let movement_type = reversal.movement_type;
    let amount_cents = reversal.amount_cents;
    let inserted = movements::insert_cash_movement(&mut *tx, reversal).await?;
    if let Some(session_key) = &session_key {
        sessions::increment_session_movements(&mut *tx, session_key, movement_type, -amount_cents)
            .await?;
    }
    let delta = -drawer_delta(movement_type, amount_cents);
    if delta != 0 {
        drawer::update_drawer_balance(&mut *tx, &terminal_id, delta).await?;
    }
    Ok(inserted)
}
