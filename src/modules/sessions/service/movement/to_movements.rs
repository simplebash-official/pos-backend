use super::category_names::category_names;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sessions::CashMovement;

use crate::modules::cash_settings;
use crate::modules::sessions::model::CashMovementDocument;
use crate::modules::sessions::repository::movements;

use std::collections::HashMap;

/// Converts stored rows to the API shape: the live category name, and the reversal of each if one exists.
pub(crate) async fn to_movements(
    db: &Db,
    docs: Vec<CashMovementDocument>,
) -> AppResult<Vec<CashMovement>> {
    // Nothing to convert, nothing to look up (a shop with no movements yet need not have its rules row).
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let names = category_names(db, &docs).await?;
    let undo_window_seconds = cash_settings::service::undo_window_seconds(db).await?;
    let keys: Vec<String> = docs.iter().map(|d| d.key.clone()).collect();
    let reversals: HashMap<String, String> = movements::find_reversals_of(db, &keys)
        .await?
        .into_iter()
        .collect();
    Ok(docs
        .into_iter()
        .map(|doc| {
            let name = names
                .get(&doc.category_key)
                .cloned()
                .expect("category_names resolves the category of every row it is given");
            let reversed_by = reversals.get(&doc.key).cloned();
            doc.into_cash_movement(name, reversed_by, undo_window_seconds)
        })
        .collect())
}
