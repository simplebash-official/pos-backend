use super::find_family::find_family;
use crate::clients::document_server::DocumentServerClient;
use crate::core::error::{AppError, AppResult};

use crate::modules::documents::kinds;

/// PDF of a design's own sample data, for the Settings preview. Only designs
/// a shop can actually choose are previewable.
pub(crate) async fn render_preview(
    client: &DocumentServerClient,
    family: &str,
) -> AppResult<Vec<u8>> {
    let templates = client.list_templates().await?;
    let info = kinds::SELECTABLE_KINDS
        .iter()
        .find_map(|kind| find_family(&templates, kind.slug, family))
        .ok_or_else(|| {
            AppError::not_found_with_code(
                "That design is not available.".to_string(),
                "TEMPLATE_UNAVAILABLE",
            )
        })?;
    client.render_sample(info).await
}
