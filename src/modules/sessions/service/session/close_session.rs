use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::{
    CloseSessionRequest, RegisterSession, SessionStatus, VARIANCE_TOLERANCE_CENTS,
};

use crate::modules::sessions::model::DrawerStateDocument;
use crate::modules::sessions::repository::{drawer, sessions};
use crate::modules::users;
use mongodb::bson::DateTime as BsonDateTime;

pub async fn close_session(
    db: &Db,
    session_key: &str,
    cashier_id: String,
    cashier_name: String,
    body: CloseSessionRequest,
) -> AppResult<RegisterSession> {
    if body.counted_cash_cents < 0 {
        return Err(AppError::validation("countedCashCents cannot be negative"));
    }
    if body.closing_float_cents < 0 {
        return Err(AppError::validation("closingFloatCents cannot be negative"));
    }
    let safe_deposit = body.cash_drop_to_safe_cents.unwrap_or(0);
    if safe_deposit < 0 {
        return Err(AppError::validation(
            "cashDropToSafeCents cannot be negative",
        ));
    }

    if body.closing_float_cents + safe_deposit != body.counted_cash_cents {
        return Err(AppError::validation(format!(
            "Closing float ({} cents) + safe deposit ({} cents) must equal counted cash ({} cents)",
            body.closing_float_cents, safe_deposit, body.counted_cash_cents
        )));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    if session.status == SessionStatus::Closed {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_CLOSED,
            "Register session is already closed",
        ));
    }

    let expected_cash = session.calculate_expected_cash();
    let variance = body.counted_cash_cents - expected_cash;
    let mut variance_approved_by_id = None;
    let mut variance_approved_by_name = None;

    if variance != 0 {
        if body
            .variance_reason
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err(AppError::validation_with_code(
                format!(
                    "Cash variance of {} cents requires a variance explanation reason",
                    variance
                ),
                codes::VARIANCE_APPROVAL_REQUIRED,
            ));
        }

        // If variance is outside small tolerance, require manager PIN approval
        if variance.abs() > VARIANCE_TOLERANCE_CENTS {
            let pin = body.manager_pin.as_deref().ok_or_else(|| {
                AppError::forbidden_with_code(
                    "Manager PIN is required to approve cash variance exceeding tolerance",
                    codes::VARIANCE_APPROVAL_REQUIRED,
                )
            })?;

            let approver = users::service::verify_user_pin(db, pin, Some("manager"))
                .await?
                .ok_or_else(|| {
                    AppError::unauthorized_with_code("Invalid manager PIN", codes::INVALID_PIN)
                })?;

            variance_approved_by_id = Some(approver.id);
            variance_approved_by_name = Some(approver.name);
        }
    }

    let now = BsonDateTime::now();
    session.closed_at = Some(now);
    session.closing_cashier_id = Some(cashier_id);
    session.closing_cashier_name_snapshot = Some(cashier_name);
    session.expected_cash_cents = Some(expected_cash);
    session.counted_cash_cents = Some(body.counted_cash_cents);
    session.cash_variance_cents = Some(variance);
    session.closing_denominations = body.closing_denominations;
    session.closing_note = body.closing_note;
    session.closing_float_cents = Some(body.closing_float_cents);
    session.cash_drop_to_safe_cents = Some(safe_deposit);
    session.variance_reason = body.variance_reason;
    session.variance_approved_by_id = variance_approved_by_id;
    session.variance_approved_by_name = variance_approved_by_name;
    session.status = SessionStatus::Closed;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to update register session status to closed"))?;

    // Update drawer ledger: retains the specified closing float for next shift
    drawer::upsert_drawer_state(
        db,
        DrawerStateDocument {
            terminal_id: session.terminal_id.clone(),
            current_balance_cents: body.closing_float_cents,
            last_session_key: Some(session.key.clone()),
            last_closing_float_cents: body.closing_float_cents,
            updated_at: now,
        },
    )
    .await?;

    Ok(updated.into_register_session())
}
