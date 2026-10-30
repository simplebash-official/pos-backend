use crate::clients::document_server::TemplateInfo;

/// The offered template of design `family` for `kind`, if any.
pub(super) fn find_family<'a>(
    templates: &'a [TemplateInfo],
    kind: &str,
    family: &str,
) -> Option<&'a TemplateInfo> {
    templates
        .iter()
        .find(|template| template.kind == kind && template.family == family)
}
