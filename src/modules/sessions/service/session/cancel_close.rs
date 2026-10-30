use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::{RegisterSession, SessionStatus};

use crate::modules::sessions::repository::sessions;

/// Backs out of a started close: a counted ("closing") shift goes back to open (or reopened, if it
/// had been reopened) and the recorded count is cleared. Open and reopened shifts are left alone.
pub async fn cancel_close(db: &Db, session_key: &str) -> AppResult<RegisterSession> {
    let mut session = sessions::find_session_by_key(db, session_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;

    match session.status {
        SessionStatus::Open | SessionStatus::Reopened => return Ok(session.into_register_session()),
        SessionStatus::Closed => {
            return Err(AppError::conflict(
                codes::SESSION_ALREADY_CLOSED,
                "Cannot cancel closing of an already closed session",
            ));
        }
        SessionStatus::Closing => {}
    }

    session.status = if session.reopened_at.is_some() {
        SessionStatus::Reopened
    } else {
        SessionStatus::Open
    };
    session.expected_cash_cents = None;
    session.counted_cash_cents = None;
    session.cash_variance_cents = None;
    session.closing_denominations = None;

    let updated = sessions::update_session(db, &session)
        .await?
        .ok_or_else(|| AppError::internal("Failed to cancel closing of register session"))?;

    Ok(updated.into_register_session())
}
