use super::default_for::default_for;
use super::design_settings::DesignSettings;
use super::effective_options::effective_options;
use super::find_family::find_family;
use super::resolved_template::ResolvedTemplate;
use super::unavailable::unavailable;
use crate::clients::document_server::TemplateInfo;
use crate::core::error::{AppError, AppResult};

use crate::modules::documents::kinds;

pub(crate) fn resolve_in(
    templates: &[TemplateInfo],
    kind: &str,
    requested: Option<&str>,
    settings: &DesignSettings,
) -> AppResult<ResolvedTemplate> {
    let kind_def = kinds::find(kind)
        .ok_or_else(|| AppError::internal(format!("unknown document kind '{kind}'")))?;
    let default = default_for(templates, kind)?;

    let chosen = match requested.or_else(|| settings.selections.get(kind).map(String::as_str)) {
        Some(family) => find_family(templates, kind, family).ok_or_else(unavailable)?,
        None => default,
    };

    Ok(ResolvedTemplate {
        kind: kind_def.slug,
        is_default: chosen.family == default.family,
        option_values: effective_options(chosen, settings.options.get(&chosen.family))?,
        info: chosen.clone(),
    })
}
