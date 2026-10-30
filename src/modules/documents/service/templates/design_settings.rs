use super::option_values::OptionValues;
use super::selections::Selections;

/// Everything stored about a shop's designs (`document_template_settings`).
#[derive(Debug, Clone, Default)]
pub(crate) struct DesignSettings {
    pub selections: Selections,
    pub options: OptionValues,
}
