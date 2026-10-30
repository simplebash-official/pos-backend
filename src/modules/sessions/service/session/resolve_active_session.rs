use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::RegisterSessionDocument;
use crate::modules::sessions::repository::sessions;

pub async fn resolve_active_session(
    db: &Db,
    terminal_id: Option<&str>,
    cashier_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    let profile = crate::modules::settings::service::get_shop_profile(db, None).await?;
    let term = terminal_id.map(str::trim).filter(|t| !t.is_empty());

    if profile.allow_shared_user_sessions {
        if let Some(t) = term
            && let Some(s) = sessions::find_active_session_for_terminal(db, t).await?
        {
            return Ok(Some(s));
        }
        sessions::find_active_session_for_cashier(db, cashier_id).await
    } else {
        if let Some(t) = term {
            sessions::find_active_session_for_terminal(db, t).await
        } else {
            Ok(None)
        }
    }
}
