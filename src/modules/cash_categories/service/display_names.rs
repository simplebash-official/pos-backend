use super::resolve_categories::resolve_categories;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use std::collections::HashMap;

/// Live display names for category keys (see [`resolve_categories`]).
pub(crate) async fn display_names(db: &Db, keys: &[String]) -> AppResult<HashMap<String, String>> {
    Ok(resolve_categories(db, keys)
        .await?
        .into_iter()
        .map(|(key, resolved)| (key, resolved.name))
        .collect())
}
