use super::populate_missing_cashier_name::populate_missing_cashier_name;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::RegisterSession;

use crate::modules::sessions::repository::sessions;

pub async fn get_active_session_for_cashier(
    db: &Db,
    cashier_id: &str,
) -> AppResult<Option<RegisterSession>> {
    let session = sessions::find_active_session_for_cashier(db, cashier_id).await?;
    match session {
        Some(s) => {
            let mut reg = s.into_register_session();
            populate_missing_cashier_name(db, &mut reg).await;
            Ok(Some(reg))
        }
        None => Ok(None),
    }
}
