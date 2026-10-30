use super::get_or_render::get_or_render;
use super::get_original::get_original;

use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;

use crate::core::error::AppResult;

use crate::domain::settings::DocumentDesignChoice;

use crate::modules::settings;

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
            let resolved =
                super::templates::resolve(doc_server, kind, requested, &settings).await?;
            let data = super::templates::with_options(data, &resolved)?;
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
