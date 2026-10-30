use crate::clients::document_server::TemplateInfo;
use crate::core::error::{AppError, AppResult};

/// The kind's built-in default template.
pub(super) fn default_for<'a>(
    templates: &'a [TemplateInfo],
    kind: &str,
) -> AppResult<&'a TemplateInfo> {
    templates
        .iter()
        .find(|template| template.kind == kind && template.is_default)
        .ok_or_else(|| {
            AppError::internal(format!(
                "document-server publishes no default template for kind '{kind}'"
            ))
        })
}
