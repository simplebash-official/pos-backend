use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::{RegisterSession, SessionStatus, SubmitBlindCountRequest};

use crate::modules::sessions::repository::sessions;

pub async fn submit_blind_count(
    db: &Db,
    session_key: &str,
    body: SubmitBlindCountRequest,
) -> AppResult<RegisterSession> {
    if body.counted_cash_cents < 0 {
        return Err(AppError::validation("countedCashCents cannot be negative"));
    }

    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    if session.status != SessionStatus::Open
        && session.status != SessionStatus::Closing
        && session.status != SessionStatus::Reopened
    {
        return Err(AppError::conflict(
            codes::SESSION_ALREADY_CLOSED,
            "Cannot submit count for an already closed session",
        ));
    }

    let expected_cash = session.calculate_expected_cash();
    let variance = body.counted_cash_cents - expected_cash;

    session.expected_cash_cents = Some(expected_cash);
    session.counted_cash_cents = Some(body.counted_cash_cents);
    session.cash_variance_cents = Some(variance);
    session.closing_denominations = body.closing_denominations;
    session.status = SessionStatus::Closing;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to save blind count on session"))?;

    Ok(updated.into_register_session())
}
