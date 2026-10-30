use super::is_safe_path_segment::is_safe_path_segment;
use super::templates::ResolvedTemplate;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;

use crate::modules::documents::model::GeneratedDocumentDocument;
use crate::modules::documents::repository;

use mongodb::bson::DateTime as BsonDateTime;
use std::path::Path;

/// Returns the PDF bytes for `(entity_key, cache key)`, rendering and
/// persisting a fresh copy only if one doesn't already exist.
///
/// `legacy_cache_key` discriminates two renderings that must never be
/// confused for caching purposes (e.g. `billing::routes` uses
/// `"thermal-receipt-80mm"` vs `"thermal-receipt-58mm"` so a terminal's
/// paper-width choice, D9, can't serve a cached PDF sized for the wrong
/// roll). The key actually stored is `templates::cache_key`: the same value
/// for the kind's built-in default template (so PDFs cached before templates
/// were selectable stay valid), and a template-specific one otherwise, so one
/// design is never served from another's cache. `resolved.kind` (e.g.
/// `"a4-invoice"`) is persisted as the row's `template_name` — document-server
/// has no concept of a width variant, that's encoded in `data.paperWidthMm`
/// instead — and the template's own `tpl_...` key as `template_key`.
///
/// Invoices are immutable once created (see `modules::billing`'s design —
/// no general edit endpoint), so caching by entity+cache_key is safe: the
/// data that produced a rendering never changes underneath it. If the
/// Mongo row exists but the file is missing from disk (an operator cleared
/// the folder), this falls through and re-renders rather than erroring —
/// the row will end up superseded, not corrected, since nothing here
/// deletes the stale metadata row; that's an acceptable, easily-cleaned-up
/// inconsistency rather than a reason to fail a print request.
pub(crate) async fn get_or_render(
    db: &Db,
    doc_server: &DocumentServerClient,
    generated_documents_dir: &str,
    entity_key: &str,
    legacy_cache_key: &str,
    resolved: &ResolvedTemplate,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    let cache_key = super::templates::cache_key(legacy_cache_key, resolved);
    let cache_key = cache_key.as_str();

    // Both keys become path segments under `generated_documents_dir`. Entity
    // keys normally look like `inv_<nanoid>`, but a sync push can create an
    // invoice/credit note with any key string, so a `../` or absolute key
    // must never reach `Path::join`.
    if !is_safe_path_segment(entity_key) || !is_safe_path_segment(cache_key) {
        return Err(crate::core::error::AppError::validation(
            "document key contains characters that are not allowed",
        ));
    }

    if let Some(existing) =
        repository::find_latest_by_entity_and_type(db, entity_key, cache_key).await?
    {
        let full_path = Path::new(generated_documents_dir).join(&existing.file_path);
        match tokio::fs::read(&full_path).await {
            Ok(bytes) => return Ok(bytes),
            Err(err) => {
                tracing::warn!(
                    entity_key,
                    cache_key,
                    path = %full_path.display(),
                    error = %err,
                    "generated_documents row exists but the file is missing on disk — re-rendering"
                );
            }
        }
    }

    let pdf_bytes = doc_server.render_template(&resolved.info, data).await?;

    let file_name = format!(
        "{cache_key}_{}_{}.pdf",
        chrono::Utc::now().timestamp_millis(),
        nanoid::nanoid!(8)
    );
    let relative_path = format!("{entity_key}/{file_name}");
    let entity_dir = Path::new(generated_documents_dir).join(entity_key);

    tokio::fs::create_dir_all(&entity_dir)
        .await
        .map_err(|err| {
            crate::core::error::AppError::internal(format!(
                "failed to create directory for generated document: {err}"
            ))
        })?;
    tokio::fs::write(entity_dir.join(&file_name), &pdf_bytes)
        .await
        .map_err(|err| {
            crate::core::error::AppError::internal(format!(
                "failed to save generated document to disk: {err}"
            ))
        })?;

    let now = BsonDateTime::now();
    repository::insert(
        db,
        GeneratedDocumentDocument {
            id: None,
            key: generate_id(prefixes::GENERATED_DOCUMENT),
            entity_key: entity_key.to_string(),
            document_type: cache_key.to_string(),
            template_name: resolved.kind.to_string(),
            template_key: resolved.info.key.clone(),
            template_ref: Some(resolved.info.name.clone()),
            template_title: Some(resolved.info.title.clone()),
            template_version: Some(resolved.info.version),
            file_path: relative_path,
            file_size_bytes: pdf_bytes.len() as i64,
            created_at: now,
            updated_at: now,
        },
    )
    .await?;

    Ok(pdf_bytes)
}
