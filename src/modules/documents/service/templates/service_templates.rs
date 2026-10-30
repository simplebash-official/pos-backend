use crate::clients::document_server::{DocumentServerClient, TemplateInfo};
use crate::core::error::{AppError, AppResult};
use crate::domain::settings::{
    DesignOption, DesignOptionType, DocumentDesignFamily, DocumentKindSettings,
};
use crate::modules::documents::kinds;
use std::collections::HashMap;

/// The `generated_documents` cache key for a print. The built-in default keeps
/// the kind's historical key, so every PDF cached before designs were
/// selectable stays valid; any other template gets its own key (per design
/// *version*, because the name is immutable), so a design is never served
/// from another design's cache. A design with options also carries a digest of
/// the option values, so changing a setting renders afresh instead of serving
/// the PDF cached under the old values.
pub(crate) fn cache_key(legacy_key: &str, resolved: &ResolvedTemplate) -> String {
    if resolved.is_default {
        legacy_key.to_string()
    } else if resolved.option_values.is_empty() {
        format!("{legacy_key}--{}", resolved.info.name)
    } else {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(
            serde_json::Value::Object(resolved.option_values.clone())
                .to_string()
                .as_bytes(),
        );
        let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
        format!("{legacy_key}--{}--{short}", resolved.info.name)
    }
}

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

/// Everything stored about a shop's designs (`document_template_settings`).
#[derive(Debug, Clone, Default)]
pub(crate) struct DesignSettings {
    pub selections: Selections,
    pub options: OptionValues,
}

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

pub(super) fn invalid_option(message: String) -> AppError {
    AppError::unprocessable_entity("TEMPLATE_OPTIONS_INVALID", message)
}

pub(crate) async fn list(
    client: &DocumentServerClient,
    settings: &DesignSettings,
) -> AppResult<Vec<DocumentKindSettings>> {
    let templates = client.list_templates().await?;
    list_in(&templates, settings)
}

/// What the Settings screen offers per kind. A kind whose built-in default is
/// not published is a document-server deployment fault and fails the listing.
pub(crate) fn list_in(
    templates: &[TemplateInfo],
    settings: &DesignSettings,
) -> AppResult<Vec<DocumentKindSettings>> {
    kinds::SELECTABLE_KINDS
        .iter()
        .map(|kind| {
            let default = default_for(templates, kind.slug)?;
            let mut families: Vec<DocumentDesignFamily> = templates
                .iter()
                .filter(|template| template.kind == kind.slug)
                .map(|template| DocumentDesignFamily {
                    family: template.family.clone(),
                    title: template.title.clone(),
                    version: template.version,
                    is_default: template.family == default.family,
                    options: template.options.clone(),
                })
                .collect();
            families.sort_by(|a, b| {
                b.is_default
                    .cmp(&a.is_default)
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                    .then_with(|| a.family.cmp(&b.family))
            });

            let selected = settings.selections.get(kind.slug).cloned();
            let effective = match &selected {
                Some(family) => find_family(templates, kind.slug, family).map(|t| t.family.clone()),
                None => Some(default.family.clone()),
            };

            let mut option_values = HashMap::new();
            for template in templates.iter().filter(|t| t.kind == kind.slug) {
                if template.options.is_empty() {
                    continue;
                }
                let values = effective_options(template, settings.options.get(&template.family))?;
                option_values.insert(
                    template.family.clone(),
                    values.into_iter().collect::<HashMap<_, _>>(),
                );
            }

            Ok(DocumentKindSettings {
                kind: kind.slug.to_string(),
                label: kind.label.to_string(),
                selected,
                effective,
                families,
                option_values,
            })
        })
        .collect()
}

pub(crate) async fn merge_settings(
    client: &DocumentServerClient,
    current: DesignSettings,
    selection_update: &HashMap<String, Option<String>>,
    option_update: &OptionValues,
) -> AppResult<DesignSettings> {
    let templates = client.list_templates().await?;
    merge_settings_in(&templates, current, selection_update, option_update)
}

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

/// A shop's saved option values: design family -> option key -> value.
pub(crate) type OptionValues = HashMap<String, HashMap<String, serde_json::Value>>;

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

pub(crate) async fn resolve(
    client: &DocumentServerClient,
    kind: &str,
    requested: Option<&str>,
    settings: &DesignSettings,
) -> AppResult<ResolvedTemplate> {
    let templates = client.list_templates().await?;
    resolve_in(&templates, kind, requested, settings)
}

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

/// A shop's stored choices: document kind slug -> design family.
pub(crate) type Selections = HashMap<String, String>;

pub(super) fn unavailable() -> AppError {
    AppError::unprocessable_entity(
        "TEMPLATE_UNAVAILABLE",
        "That design is not available for this document. Choose another design in Settings.",
    )
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Checks one value against the control that declared it.
pub(crate) fn validate_option_value(
    option: &DesignOption,
    value: &serde_json::Value,
) -> Result<(), String> {
    let label = &option.label;
    match option.r#type {
        DesignOptionType::Toggle if value.is_boolean() => Ok(()),
        DesignOptionType::Toggle => Err(format!("'{label}' must be on or off")),
        DesignOptionType::Select => {
            let picked = value.as_str().unwrap_or_default();
            let known = option
                .choices
                .as_ref()
                .is_some_and(|choices| choices.iter().any(|choice| choice.value == picked));
            if known {
                Ok(())
            } else {
                Err(format!("'{label}' must be one of the listed choices"))
            }
        }
        DesignOptionType::Color if value.as_str().is_some_and(is_hex_color) => Ok(()),
        DesignOptionType::Color => Err(format!("'{label}' must be a colour like #1A2B3C")),
        DesignOptionType::Text => {
            let max = option.max_length.unwrap_or(0) as usize;
            match value.as_str() {
                Some(text) if text.chars().count() <= max => Ok(()),
                Some(_) => Err(format!("'{label}' can be at most {max} characters")),
                None => Err(format!("'{label}' must be text")),
            }
        }
    }
}

/// Adds the payload's `options` object for a design that declares options
/// (its schema requires it); a design without options gets none.
pub(crate) fn with_options(
    mut data: serde_json::Value,
    resolved: &ResolvedTemplate,
) -> AppResult<serde_json::Value> {
    if resolved.info.options.is_empty() {
        return Ok(data);
    }
    match data.as_object_mut() {
        Some(fields) => {
            fields.insert(
                "options".to_string(),
                serde_json::Value::Object(resolved.option_values.clone()),
            );
            Ok(data)
        }
        None => Err(AppError::internal("print payload is not a JSON object")),
    }
}
