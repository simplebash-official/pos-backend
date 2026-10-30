use super::is_safe_path_segment::is_safe_path_segment;

use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::documents::repository;

use std::path::Path;

fn no_original() -> crate::core::error::AppError {
    crate::core::error::AppError::not_found_with_code(
        "This document has not been printed before, so there is no original to print.",
        "NO_ORIGINAL_DOCUMENT",
    )
}

/// The PDF as it was first printed: the newest stored document of this print
/// variant (`legacy_cache_key`) whatever design rendered it. 404
/// `NO_ORIGINAL_DOCUMENT` when nothing was stored or its file is gone.
pub(crate) async fn get_original(
    db: &Db,
    generated_documents_dir: &str,
    entity_key: &str,
    legacy_cache_key: &str,
) -> AppResult<Vec<u8>> {
    if !is_safe_path_segment(entity_key) || !is_safe_path_segment(legacy_cache_key) {
        return Err(crate::core::error::AppError::validation(
            "document key contains characters that are not allowed",
        ));
    }
    let stored = repository::find_latest_by_entity_and_variant(db, entity_key, legacy_cache_key)
        .await?
        .ok_or_else(no_original)?;
    tokio::fs::read(Path::new(generated_documents_dir).join(&stored.file_path))
        .await
        .map_err(|_| no_original())
}
