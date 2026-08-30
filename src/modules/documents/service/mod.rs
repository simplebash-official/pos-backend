// Orchestrates "give me the PDF for this entity" — check for an existing
// rendering first (cache-by-`(entity_key, document_type)`), otherwise call
// the document-server, save the bytes to disk, and record the metadata.
// This is the only module allowed to call `clients::document_server`
// directly (see `document_server::DocumentServerClient`'s module comment).

use std::path::Path;

use mongodb::{Database, bson::DateTime as BsonDateTime};

use crate::{
    clients::document_server::DocumentServerClient,
    core::{constants::prefixes, error::AppResult, id::generate_id},
    modules::documents::{model::GeneratedDocumentDocument, repository},
};

/// Renders a template through the document-server without caching or writing
/// a `generated_documents` row. For **dynamic** documents whose source data
/// changes between renders and that have no owning entity to key a cache on —
/// today just the Analytics & Reports PDF (`GET /reports/analytics/document`),
/// where a report over "this month" must reflect every sale rung since the
/// last render. Still routed through this module so `clients::document_server`
/// keeps its single-caller invariant.
pub(crate) async fn render_uncached(
    doc_server: &DocumentServerClient,
    template_slug: &str,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    doc_server.render(template_slug, data).await
}

/// Returns the PDF bytes for `(entity_key, cache_key)`, rendering and
/// persisting a fresh copy only if one doesn't already exist. `cache_key`
/// and `template_name` are deliberately separate: `cache_key` is whatever
/// discriminates two renderings that must never be confused for caching
/// purposes (e.g. `billing::routes` uses `"thermal-receipt-80mm"` vs
/// `"thermal-receipt-58mm"` so a terminal's paper-width choice, D9, can't
/// serve a cached PDF sized for the wrong roll), while `template_name` is
/// the stable document-server template *slug* to render (`"a4-invoice"` /
/// `"thermal-receipt"` — document-server has no concept of a width variant,
/// that's encoded in `data.paperWidthMm` instead). The slug is what's
/// persisted on the row; `clients::document_server` maps it to the
/// template's current `description` for the wire lookup.
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
    db: &Database,
    doc_server: &DocumentServerClient,
    generated_documents_dir: &str,
    entity_key: &str,
    cache_key: &str,
    template_name: &str,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
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

    let pdf_bytes = doc_server.render(template_name, data).await?;
    // The document-server-minted unique template key, resolved from the
    // client's (now warm) name→info cache — recorded so this row states not
    // just *which* template layout but *which ID the document server gave
    // it*, per the integration contract.
    let template_key = doc_server.resolve_template_key(template_name).await?;

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
            template_name: template_name.to_string(),
            template_key,
            file_path: relative_path,
            file_size_bytes: pdf_bytes.len() as i64,
            created_at: now,
            updated_at: now,
        },
    )
    .await?;

    Ok(pdf_bytes)
}
