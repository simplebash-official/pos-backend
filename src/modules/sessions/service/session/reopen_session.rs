use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::{RegisterSession, ReopenSessionRequest, SessionStatus};
use crate::domain::users::Role;
use crate::modules::sessions::model::DrawerStateDocument;
use crate::modules::sessions::repository::{drawer, sessions};

use mongodb::bson::DateTime as BsonDateTime;

pub async fn reopen_session(
    db: &Db,
    session_key: &str,
    user_id: String,
    user_name: String,
    user_role: Option<Role>,
    body: ReopenSessionRequest,
) -> AppResult<RegisterSession> {
    if user_role != Some(Role::Admin) && user_role != Some(Role::Manager) {
        return Err(AppError::forbidden_with_code(
            "Only managers and admins can reopen a closed session",
            codes::PERMISSION_DENIED,
        ));
    }
    if body.reason.trim().is_empty() {
        return Err(AppError::validation("Reopen reason is required"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    if session.status != SessionStatus::Closed {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            "Only closed sessions can be reopened",
        ));
    }

    // Ensure no other active session has been opened on this terminal
    if let Some(existing) =
        sessions::find_active_session_for_terminal(db, &session.terminal_id).await?
        && existing.key != session.key
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            format!(
                "Cannot reopen session: an active register session ({}) is already open on terminal '{}'",
                existing.session_number, session.terminal_id
            ),
        ));
    }

    // Ensure this cashier does not already have another active session
    if let Some(existing) =
        sessions::find_active_session_for_cashier(db, &session.cashier_id).await?
        && existing.key != session.key
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_OPEN,
            format!(
                "Cannot reopen session: cashier already has an active register session ({}) open",
                existing.session_number
            ),
        ));
    }

    let now = BsonDateTime::now();
    session.status = SessionStatus::Reopened;
    session.closed_at = None;
    session.closing_cashier_id = None;
    session.closing_cashier_name_snapshot = None;
    session.expected_cash_cents = None;
    session.counted_cash_cents = None;
    session.cash_variance_cents = None;
    session.closing_denominations = None;
    session.closing_float_cents = None;
    session.cash_drop_to_safe_cents = None;
    session.variance_reason = None;
    session.variance_approved_by_id = None;
    session.variance_approved_by_name = None;
    session.closing_note = None;
    session.reopened_at = Some(now);
    session.reopened_by_id = Some(user_id);
    session.reopened_by_name = Some(user_name);
    session.reopened_reason = Some(body.reason);

    // Restore drawer ledger balance to expected cash for resumed operation
    let expected_cash = session.calculate_expected_cash();
    drawer::upsert_drawer_state(
        db,
        DrawerStateDocument {
            terminal_id: session.terminal_id.clone(),
            current_balance_cents: expected_cash,
            last_session_key: Some(session.key.clone()),
            last_closing_float_cents: session.opening_float_cents,
            updated_at: now,
        },
    )
    .await?;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to reopen register session"))?;

    Ok(updated.into_register_session())
}
