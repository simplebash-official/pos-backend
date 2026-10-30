use super::populate_missing_cashier_name::populate_missing_cashier_name;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::RegisterSession;

use crate::modules::sessions::repository::sessions;

pub async fn get_session(db: &Db, key: &str) -> AppResult<RegisterSession> {
    let session = sessions::find_session_by_id_or_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND)
        })?;
    let mut reg = session.into_register_session();
    populate_missing_cashier_name(db, &mut reg).await;
    Ok(reg)
}
