use crate::clients::db::Db;

use crate::domain::sessions::RegisterSession;

use crate::modules::users;

/// If a session's `cashier_name_snapshot` is empty (e.g. from an older session opened
/// when cashier names were not propagated), resolve it from `users::service::get_user`.
pub async fn populate_missing_cashier_name(db: &Db, session: &mut RegisterSession) {
    if session.cashier_name_snapshot.trim().is_empty()
        && let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(&session.cashier_id)
        && let Ok(user) = users::service::get_user(db, oid).await
    {
        let trimmed = user.name.trim();
        if !trimmed.is_empty() {
            session.cashier_name_snapshot = trimmed.to_string();
        }
    }
    if let Some(ref closing_id) = session.closing_cashier_id
        && session
            .closing_cashier_name_snapshot
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        && let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(closing_id)
        && let Ok(user) = users::service::get_user(db, oid).await
    {
        let trimmed = user.name.trim();
        if !trimmed.is_empty() {
            session.closing_cashier_name_snapshot = Some(trimmed.to_string());
        }
    }
}
