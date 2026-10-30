use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::cash_categories::repository;

/// `key` plus every category merged into it, for filtering movements by a category.
pub(crate) async fn keys_including_merged(db: &Db, key: &str) -> AppResult<Vec<String>> {
    let mut keys: Vec<String> = repository::list_all(db)
        .await?
        .into_iter()
        .filter(|c| c.merged_into_key.as_deref() == Some(key))
        .map(|c| c.key)
        .collect();
    keys.push(key.to_string());
    Ok(keys)
}
