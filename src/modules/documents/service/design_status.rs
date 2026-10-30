use super::is_safe_path_segment::is_safe_path_segment;

use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::settings::{CurrentDocumentDesign, DocumentDesignStatus, StoredDocumentDesign};

use crate::modules::documents::repository;

/// What the stored PDF of this print variant was printed with versus what
/// would print now (`resolved`). `differs` is only true when the stored row
/// recorded its design and it is another template than the current one — a
/// row stored before designs were recorded cannot be compared.
pub(crate) async fn design_status(
    db: &Db,
    entity_key: &str,
    legacy_cache_key: &str,
    resolved: &super::templates::ResolvedTemplate,
) -> AppResult<DocumentDesignStatus> {
    if !is_safe_path_segment(entity_key) || !is_safe_path_segment(legacy_cache_key) {
        return Err(crate::core::error::AppError::validation(
            "document key contains characters that are not allowed",
        ));
    }
    let stored = repository::find_latest_by_entity_and_variant(db, entity_key, legacy_cache_key)
        .await?
        .map(|row| StoredDocumentDesign {
            template_ref: row.template_ref,
            title: row.template_title,
            version: row.template_version,
            created_at: row.created_at.to_chrono(),
        });
    let differs = stored
        .as_ref()
        .and_then(|stored| stored.template_ref.as_deref())
        .is_some_and(|stored_ref| stored_ref != resolved.info.name);
    Ok(DocumentDesignStatus {
        stored,
        current: CurrentDocumentDesign {
            template_ref: resolved.info.name.clone(),
            title: resolved.info.title.clone(),
            version: resolved.info.version,
        },
        differs,
    })
}
