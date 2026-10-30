use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::CashMovementDocument;

use crate::modules::cash_categories;

use std::collections::HashMap;

/// Live category names for a batch of movement rows.
pub(crate) async fn category_names(
    db: &Db,
    docs: &[CashMovementDocument],
) -> AppResult<HashMap<String, String>> {
    let mut keys: Vec<String> = docs.iter().map(|d| d.category_key.clone()).collect();
    keys.sort_unstable();
    keys.dedup();
    cash_categories::service::display_names(db, &keys).await
}
