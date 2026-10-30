use crate::clients::document_server::DocumentServerClient;
use crate::core::error::AppResult;

use crate::modules::documents::service::templates;

/// PDF of one design's sample data, for the Settings preview.
pub(crate) async fn render_document_template_preview(
    doc_server: &DocumentServerClient,
    family: &str,
) -> AppResult<Vec<u8>> {
    templates::render_preview(doc_server, family).await
}
