use super::find_template::find_template;
use super::has_price_element::has_price_element;

use super::render::render;
use super::resolve_stickers::resolve_stickers;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;

use crate::core::error::AppResult;

use crate::domain::labels::RenderLabelsRequest;

/// Prints the chosen items with a saved design or starter: one PDF page per sticker.
pub(crate) async fn render_labels(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    request: RenderLabelsRequest,
) -> AppResult<Vec<u8>> {
    let template = find_template(db, &request.template_key).await?;
    let stickers =
        resolve_stickers(db, has_price_element(&template.elements), &request.items).await?;
    render(
        db,
        doc_server,
        tenant_id,
        template.width_mm,
        template.height_mm,
        &template.elements,
        &stickers,
    )
    .await
}
