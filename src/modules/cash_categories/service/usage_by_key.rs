use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::sessions;
use std::collections::HashMap;

pub(super) async fn usage_by_key(db: &Db) -> AppResult<HashMap<String, i64>> {
    Ok(sessions::service::movement_counts_by_category(db)
        .await?
        .into_iter()
        .collect())
}
