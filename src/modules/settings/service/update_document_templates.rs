use super::document_template_settings_key::DOCUMENT_TEMPLATE_SETTINGS_KEY;
use super::persist_document_template_settings::persist_document_template_settings;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::core::error::AppResult;
use crate::domain::settings::{DocumentTemplatesResponse, UpdateDocumentTemplatesRequest};
use crate::modules::documents::service::templates;
use crate::modules::documents::service::templates::DesignSettings;

use crate::modules::settings::repository;

/// Changes the shop's designs and option values. Every pick and value is
/// validated against the document-server's current templates first; nothing
/// is saved unless all of it is valid.
pub(crate) async fn update_document_templates(
    db: &Db,
    doc_server: &DocumentServerClient,
    req: UpdateDocumentTemplatesRequest,
) -> AppResult<DocumentTemplatesResponse> {
    crate::core::logging::domain::tracked("settings.document_templates_updated", async move {
        let existing =
            repository::find_document_template_settings(db, DOCUMENT_TEMPLATE_SETTINGS_KEY).await?;
        let current = existing
            .as_ref()
            .map(|doc| DesignSettings {
                selections: doc.selections.clone(),
                options: doc.options.clone(),
            })
            .unwrap_or_default();

        let merged =
            templates::merge_settings(doc_server, current, &req.selections, &req.options).await?;

        persist_document_template_settings(db, existing, &merged).await?;
        Ok(DocumentTemplatesResponse {
            kinds: templates::list(doc_server, &merged).await?,
        })
    })
    .await
}
