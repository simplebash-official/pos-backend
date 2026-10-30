use super::super::movement;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::sessions::{CashMovement, CashMovementType, ConvertVarianceRequest};

use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::{movements, sessions};
use crate::modules::{cash_categories, sequences, users};
use mongodb::bson::DateTime as BsonDateTime;

pub async fn convert_variance(
    db: &Db,
    session_key: &str,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: ConvertVarianceRequest,
    device_id: Option<String>,
) -> AppResult<CashMovement> {
    if body.reason.trim().is_empty() {
        return Err(AppError::validation("Reason is required"));
    }
    if body.recipient.trim().is_empty() {
        return Err(AppError::validation("Recipient is required"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    let variance = session.cash_variance_cents.unwrap_or(0);
    if variance >= 0 {
        return Err(AppError::conflict(
            codes::VARIANCE_APPROVAL_REQUIRED,
            "Cannot convert variance: session does not have a cash shortage",
        ));
    }

    let shortage_amount = variance.abs();

    // Verify PIN: if shortage amount exceeds threshold, require owner/admin specifically
    let approver = users::service::verify_user_pin(db, &body.manager_pin, Some("manager"))
        .await?
        .ok_or_else(|| AppError::unauthorized_with_code("Invalid PIN", codes::INVALID_PIN))?;

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
    let movement_doc = CashMovementDocument {
        id: None,
        key: generate_id(prefixes::CASH_MOVEMENT),
        movement_number,
        movement_type: CashMovementType::OwnerWithdrawal,
        direction: CashMovementType::OwnerWithdrawal.direction(),
        amount_cents: shortage_amount,
        session_key: Some(session.key.clone()),
        terminal_id: session.terminal_id.clone(),
        category_key: cash_categories::service::other_category_key(
            CashMovementType::OwnerWithdrawal,
        ),
        custom_label: None,
        payee_key: None,
        payee_name: Some(body.recipient),
        period_start: None,
        period_end: None,
        reference_no: None,
        note: Some(format!(
            "Late-recorded owner withdrawal from variance: {}",
            body.reason
        )),
        recorded_by_user_id,
        recorded_by_name_snapshot: recorded_by_name,
        approved_by_user_id: Some(approver.id),
        approved_by_name_snapshot: Some(approver.name),
        approval_reason: None,
        reverses_key: None,
        recorded_late: true,
        original_variance_session_key: Some(session.key.clone()),
        receipt_key: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let inserted = movements::insert_cash_movement(db, movement_doc).await?;

    // Update session: record owner withdrawal, clear variance shortage, and flag late movements
    session.total_owner_withdrawals_cents += shortage_amount;
    session.expected_cash_cents = Some(session.calculate_expected_cash());
    session.cash_variance_cents = Some(0);
    session.has_late_movements = true;
    sessions::update_session(db, &session).await?;

    Ok(movement::to_movements(db, vec![inserted]).await?.remove(0))
}
