use super::invalid_option::invalid_option;
use super::validate_option_value::validate_option_value;
use crate::clients::document_server::TemplateInfo;
use crate::core::error::AppResult;

use std::collections::HashMap;

/// The option values a print uses for `info`: what the shop saved, else the
/// design's own declared default, for every option the design declares.
pub(crate) fn effective_options(
    info: &TemplateInfo,
    stored: Option<&HashMap<String, serde_json::Value>>,
) -> AppResult<serde_json::Map<String, serde_json::Value>> {
    let mut values = serde_json::Map::new();
    for option in &info.options {
        let value = stored
            .and_then(|saved| saved.get(&option.key))
            .unwrap_or(&option.default);
        validate_option_value(option, value).map_err(|reason| {
            invalid_option(format!(
                "The saved settings for '{}' no longer fit the design: {reason}. \
                 Review them in Settings.",
                info.title
            ))
        })?;
        values.insert(option.key.clone(), value.clone());
    }
    Ok(values)
}
