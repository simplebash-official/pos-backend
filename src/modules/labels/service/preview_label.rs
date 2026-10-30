use super::payload::Sticker;
use super::render::render;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;

use crate::core::error::{AppError, AppResult};

use crate::domain::labels::{LabelSample, PreviewLabelsRequest};

/// Draws one sticker of an unsaved design with example values, for the
/// designer's live preview. Applies the same rules as a real print.
pub(crate) async fn preview_label(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    request: PreviewLabelsRequest,
) -> AppResult<Vec<u8>> {
    super::validate::validate_template(&request.template)?;
    let LabelSample {
        name,
        sku,
        category,
        price_cents,
        code,
    } = request.sample;
    if code.is_empty() || code.chars().count() > 64 {
        return Err(AppError::validation(
            "The sample code must be 1 to 64 characters.",
        ));
    }
    // A preview is a sample, never recorded, so it belongs to no product.
    let sticker = Sticker {
        product_key: String::new(),
        serial_number: None,
        name,
        sku,
        category,
        price_cents: Some(price_cents),
        code,
        copies: 1,
    };
    render(
        db,
        doc_server,
        tenant_id,
        request.template.width_mm,
        request.template.height_mm,
        &request.template.elements,
        &[sticker],
    )
    .await
}
