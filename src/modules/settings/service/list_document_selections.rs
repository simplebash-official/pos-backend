use super::get_document_template_settings::get_document_template_settings;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// The kind/family pairs of this shop's stored selections (admin console).
pub async fn list_document_selections(db: &Db) -> AppResult<Vec<(String, String)>> {
    let mut selections: Vec<(String, String)> = get_document_template_settings(db)
        .await?
        .selections
        .into_iter()
        .collect();
    selections.sort();
    Ok(selections)
}
