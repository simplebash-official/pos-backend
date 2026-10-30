use super::resolved_category::ResolvedCategory;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::cash_categories::model::CashCategoryDocument;
use crate::modules::cash_categories::repository;
use std::collections::HashMap;

/// Resolves category keys to their live name and report group, following merges, so a rename or
/// merge shows on old movements and a merged-away category reports as its target.
pub(crate) async fn resolve_categories(
    db: &Db,
    keys: &[String],
) -> AppResult<HashMap<String, ResolvedCategory>> {
    let all: HashMap<String, CashCategoryDocument> = repository::list_all(db)
        .await?
        .into_iter()
        .map(|c| (c.key.clone(), c))
        .collect();
    let mut out = HashMap::new();
    for key in keys {
        let mut current = all.get(key).ok_or_else(|| {
            AppError::internal(format!("A record points at the missing category '{key}'"))
        })?;
        while let Some(next) = current.merged_into_key.as_ref() {
            current = all.get(next).ok_or_else(|| {
                AppError::internal(format!(
                    "Category '{key}' was merged into the missing category '{next}'"
                ))
            })?;
        }
        out.insert(
            key.clone(),
            ResolvedCategory {
                name: current.name.clone(),
                report_group: current.report_group,
            },
        );
    }
    Ok(out)
}
