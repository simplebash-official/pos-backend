use super::get_document_template_settings::get_document_template_settings;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::core::error::AppResult;
use crate::domain::settings::DocumentTemplatesResponse;
use crate::modules::documents::service::templates;

/// Every selectable document kind with the designs available for it, the
/// shop's current choice and the option values in force.
pub(crate) async fn get_document_templates(
    db: &Db,
    doc_server: &DocumentServerClient,
) -> AppResult<DocumentTemplatesResponse> {
    let settings = get_document_template_settings(db).await?;
    Ok(DocumentTemplatesResponse {
        kinds: templates::list(doc_server, &settings).await?,
    })
}
