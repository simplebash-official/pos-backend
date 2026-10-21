// Orchestrates "give me the PDF for this entity" — check for an existing
// rendering first (cache-by-`(entity_key, document_type)`), otherwise call
// the document-server, save the bytes to disk, and record the metadata.
// This is the only module allowed to call `clients::document_server`
// directly (see `document_server::DocumentServerClient`'s module comment).

use std::path::Path;

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::{db::Db, document_server::DocumentServerClient},
    core::{constants::prefixes, error::AppResult, id::generate_id},
    domain::settings::{
        CurrentDocumentDesign, DocumentDesignChoice, DocumentDesignStatus, StoredDocumentDesign,
    },
    modules::{
        documents::{model::GeneratedDocumentDocument, repository},
        settings,
    },
};

pub(crate) mod templates;
use templates::ResolvedTemplate;

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
    crate::core::logging::domain::tracked("documents.rendered", async move {
        doc_server.render(template_slug, data).await
    })
    .await
}

/// Renders a document dynamically through the template resolution system (honouring the
/// shop's chosen design family and options) without reading from or writing to the
/// `generated_documents` cache. Used for active register sessions where sales continue to occur.
pub(crate) async fn render_live(
    db: &Db,
    doc_server: &DocumentServerClient,
    kind: &str,
    requested: Option<&str>,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    let settings = settings::service::get_document_template_settings(db).await?;
    let resolved = templates::resolve(doc_server, kind, requested, &settings).await?;
    let data = templates::with_options(data, &resolved)?;
    doc_server.render_template(&resolved.info, data).await
}

/// True for a non-empty, at most 128-character string made only of ASCII
/// letters, digits, `_` and `-` — the alphabet of generated keys
/// (`<prefix>_<nanoid>`) and of the fixed cache keys. Anything else (a `/`,
/// `..`, a drive letter, a NUL) is refused before it can become a path.
fn is_safe_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 128
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

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
    let cache_key = templates::cache_key(legacy_cache_key, resolved);
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

/// The PDF a document endpoint answers with. `Original` is the stored PDF
/// exactly as first printed; `Current` resolves the design now in force
/// (`requested` family, else the shop's choice, else the kind's default),
/// adds its option values to `data` and renders or serves the PDF cached for
/// that exact design and options.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn print(
    db: &Db,
    doc_server: &DocumentServerClient,
    generated_documents_dir: &str,
    kind: &str,
    requested: Option<&str>,
    design: DocumentDesignChoice,
    entity_key: &str,
    legacy_cache_key: &str,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    match design {
        DocumentDesignChoice::Original => {
            get_original(db, generated_documents_dir, entity_key, legacy_cache_key).await
        }
        DocumentDesignChoice::Current => {
            let settings = settings::service::get_document_template_settings(db).await?;
            let resolved = templates::resolve(doc_server, kind, requested, &settings).await?;
            let data = templates::with_options(data, &resolved)?;
            get_or_render(
                db,
                doc_server,
                generated_documents_dir,
                entity_key,
                legacy_cache_key,
                &resolved,
                data,
            )
            .await
        }
    }
}

/// The design status behind a document endpoint: see [`design_status`].
pub(crate) async fn print_design_status(
    db: &Db,
    doc_server: &DocumentServerClient,
    kind: &str,
    requested: Option<&str>,
    entity_key: &str,
    legacy_cache_key: &str,
) -> AppResult<DocumentDesignStatus> {
    let settings = settings::service::get_document_template_settings(db).await?;
    let resolved = templates::resolve(doc_server, kind, requested, &settings).await?;
    design_status(db, entity_key, legacy_cache_key, &resolved).await
}

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

/// What the stored PDF of this print variant was printed with versus what
/// would print now (`resolved`). `differs` is only true when the stored row
/// recorded its design and it is another template than the current one — a
/// row stored before designs were recorded cannot be compared.
pub(crate) async fn design_status(
    db: &Db,
    entity_key: &str,
    legacy_cache_key: &str,
    resolved: &templates::ResolvedTemplate,
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

#[cfg(test)]
mod tests {
    use super::is_safe_path_segment;

    #[test]
    fn only_generated_key_shapes_are_safe_path_segments() {
        for ok in [
            "inv_V1StGXR8_Z5jdHi6B-myT",
            "thermal-receipt-80mm",
            "a4-invoice",
        ] {
            assert!(is_safe_path_segment(ok), "{ok}");
        }
        for bad in [
            "",
            "..",
            "../../etc",
            "/tmp/x",
            "inv_1/..",
            "C:\\x",
            "a b",
            "inv\u{0}",
        ] {
            assert!(!is_safe_path_segment(bad), "{bad:?}");
        }
        assert!(!is_safe_path_segment(&"a".repeat(129)));
    }
}
