use super::design_settings::DesignSettings;
use super::find_family::find_family;
use super::invalid_option::invalid_option;
use super::option_values::OptionValues;
use super::unavailable::unavailable;
use super::validate_option_value::validate_option_value;
use crate::clients::document_server::TemplateInfo;
use crate::core::error::{AppError, AppResult};

use crate::modules::documents::kinds;
use std::collections::HashMap;

/// Applies an admin's update to the stored settings, validating every pick and
/// every option value. Nothing is applied unless all of it is valid.
/// `selection_update` maps a kind to a family (`None` = back to the default);
/// `option_update` replaces the given option values of a family.
pub(crate) fn merge_settings_in(
    templates: &[TemplateInfo],
    mut current: DesignSettings,
    selection_update: &HashMap<String, Option<String>>,
    option_update: &OptionValues,
) -> AppResult<DesignSettings> {
    for (kind, choice) in selection_update {
        let kind_def = kinds::find(kind)
            .ok_or_else(|| AppError::validation(format!("Unknown document kind '{kind}'")))?;
        match choice {
            None => {
                current.selections.remove(kind_def.slug);
            }
            Some(family) => {
                find_family(templates, kind_def.slug, family).ok_or_else(unavailable)?;
                current
                    .selections
                    .insert(kind_def.slug.to_string(), family.clone());
            }
        }
    }

    for (family, values) in option_update {
        let template = kinds::SELECTABLE_KINDS
            .iter()
            .find_map(|kind| find_family(templates, kind.slug, family))
            .ok_or_else(unavailable)?;
        for (key, value) in values {
            let option = template
                .options
                .iter()
                .find(|option| &option.key == key)
                .ok_or_else(|| {
                    invalid_option(format!(
                        "'{}' has no setting called '{key}'",
                        template.title
                    ))
                })?;
            validate_option_value(option, value).map_err(invalid_option)?;
        }
        current
            .options
            .entry(family.clone())
            .or_default()
            .extend(values.clone());
    }
    Ok(current)
}
