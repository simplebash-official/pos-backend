use crate::clients::document_server::TemplateInfo;

/// The template picked for one print, plus whether it is the built-in default.
#[derive(Debug)]
pub(crate) struct ResolvedTemplate {
    /// The kind's slug (what `generated_documents.template_name` stores).
    pub kind: &'static str,
    pub info: TemplateInfo,
    pub is_default: bool,
    /// The option values this print uses (empty when the design declares
    /// none); they go into the payload and into the cache key.
    pub option_values: serde_json::Map<String, serde_json::Value>,
}
