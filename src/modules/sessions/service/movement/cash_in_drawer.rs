use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::SessionStatus;

use crate::modules::sessions::repository::{drawer, sessions};

/// The cash the drawer holds right now: the open shift's expected cash, or, between shifts, the
/// drawer's own running balance.
pub(crate) async fn cash_in_drawer(
    db: &Db,
    session_key: Option<&str>,
    terminal_id: &str,
) -> AppResult<i64> {
    if let Some(session_key) = session_key {
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
                "This shift is not open, so cash cannot be recorded on it.",
                codes::NO_ACTIVE_SESSION,
            ));
        }
        Ok(session.calculate_expected_cash())
    } else {
        Ok(drawer::get_drawer_state(db, terminal_id)
            .await?
            .map_or(0, |state| state.current_balance_cents))
    }
}
