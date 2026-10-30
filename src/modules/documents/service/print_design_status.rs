use super::design_status::design_status;

use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;

use crate::core::error::AppResult;

use crate::domain::settings::DocumentDesignStatus;

use crate::modules::settings;

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
    let resolved = super::templates::resolve(doc_server, kind, requested, &settings).await?;
    design_status(db, entity_key, legacy_cache_key, &resolved).await
}
