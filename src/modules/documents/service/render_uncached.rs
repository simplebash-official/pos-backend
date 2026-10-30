use crate::clients::document_server::DocumentServerClient;

use crate::core::error::AppResult;

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
