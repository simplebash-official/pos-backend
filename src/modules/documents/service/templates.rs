// Which document-server template prints a given document. The document-server
// publishes, per tenant, the latest published version of every design family
// with the kind it is for; a shop stores one chosen *family* per kind, and a
// single print can override it. This file owns the rules (all pure functions
// over the template list, so they are unit-tested without HTTP) plus thin
// async wrappers that fetch the list.
//
// Resolution at print time (there is deliberately no silent fallback):
//   1. an explicit `?template=<family>` on the request -> must be offered
//      for the kind, else 422 TEMPLATE_UNAVAILABLE
//   2. the shop's stored family for that kind          -> must still be
//      offered, else 422 TEMPLATE_UNAVAILABLE until the shop picks again
//   3. nothing chosen                                  -> the kind's built-in
//      default (the template the document-server marks `x-default`)

use std::collections::HashMap;

use crate::{
    clients::document_server::{DocumentServerClient, TemplateInfo},
    core::error::{AppError, AppResult},
    domain::settings::{
        DesignOption, DesignOptionType, DocumentDesignFamily, DocumentKindSettings,
    },
    modules::documents::kinds,
};

/// A shop's stored choices: document kind slug -> design family.
pub(crate) type Selections = HashMap<String, String>;

/// A shop's saved option values: design family -> option key -> value.
pub(crate) type OptionValues = HashMap<String, HashMap<String, serde_json::Value>>;

/// Everything stored about a shop's designs (`document_template_settings`).
#[derive(Debug, Clone, Default)]
pub(crate) struct DesignSettings {
    pub selections: Selections,
    pub options: OptionValues,
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

fn unavailable() -> AppError {
    AppError::unprocessable_entity(
        "TEMPLATE_UNAVAILABLE",
        "That design is not available for this document. Choose another design in Settings.",
    )
}

fn invalid_option(message: String) -> AppError {
    AppError::unprocessable_entity("TEMPLATE_OPTIONS_INVALID", message)
}

/// The kind's built-in default template.
fn default_for<'a>(templates: &'a [TemplateInfo], kind: &str) -> AppResult<&'a TemplateInfo> {
    templates
        .iter()
        .find(|template| template.kind == kind && template.is_default)
        .ok_or_else(|| {
            AppError::internal(format!(
                "document-server publishes no default template for kind '{kind}'"
            ))
        })
}

/// The offered template of design `family` for `kind`, if any.
fn find_family<'a>(
    templates: &'a [TemplateInfo],
    kind: &str,
    family: &str,
) -> Option<&'a TemplateInfo> {
    templates
        .iter()
        .find(|template| template.kind == kind && template.family == family)
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

pub(crate) async fn resolve(
    client: &DocumentServerClient,
    kind: &str,
    requested: Option<&str>,
    settings: &DesignSettings,
) -> AppResult<ResolvedTemplate> {
    let templates = client.list_templates().await?;
    resolve_in(&templates, kind, requested, settings)
}

pub(crate) async fn list(
    client: &DocumentServerClient,
    settings: &DesignSettings,
) -> AppResult<Vec<DocumentKindSettings>> {
    let templates = client.list_templates().await?;
    list_in(&templates, settings)
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::settings::DesignOptionChoice;

    fn template(
        name: &str,
        family: &str,
        title: &str,
        kind: &str,
        is_default: bool,
    ) -> TemplateInfo {
        TemplateInfo {
            key: format!("tpl_{name}"),
            name: name.to_string(),
            family: family.to_string(),
            version: 1,
            kind: kind.to_string(),
            title: title.to_string(),
            r#type: "document".to_string(),
            scope: "global".to_string(),
            is_default,
            options: Vec::new(),
            data_schema: json!({}),
            sample: json!({}),
            sha256: String::new(),
            min_app_version: None,
        }
    }

    fn with_options_declared(mut info: TemplateInfo) -> TemplateInfo {
        info.scope = "private".into();
        info.options = vec![
            DesignOption {
                key: "showSign".into(),
                r#type: DesignOptionType::Toggle,
                label: "Signature box".into(),
                default: json!(true),
                choices: None,
                max_length: None,
            },
            DesignOption {
                key: "accent".into(),
                r#type: DesignOptionType::Select,
                label: "Accent".into(),
                default: json!("blue"),
                choices: Some(vec![
                    DesignOptionChoice {
                        value: "blue".into(),
                        label: "Blue".into(),
                    },
                    DesignOptionChoice {
                        value: "red".into(),
                        label: "Red".into(),
                    },
                ]),
                max_length: None,
            },
        ];
        info
    }

    fn catalog() -> Vec<TemplateInfo> {
        vec![
            template(
                "doc_temp_modern",
                "doc_temp_modern",
                "A4 Invoice Modern",
                "a4-invoice",
                false,
            ),
            template(
                "doc_temp_classic",
                "doc_temp_classic",
                "A4 Invoice",
                "a4-invoice",
                true,
            ),
            with_options_declared(template(
                "doc_temp_client2",
                "fam_client",
                "Client Invoice",
                "a4-invoice",
                false,
            )),
            template(
                "doc_temp_receipt",
                "doc_temp_receipt",
                "Thermal Receipt",
                "thermal-receipt",
                true,
            ),
            template(
                "doc_temp_credit",
                "doc_temp_credit",
                "Credit Note",
                "credit-note",
                true,
            ),
            template(
                "doc_temp_quote",
                "doc_temp_quote",
                "Quotation",
                "quotation",
                true,
            ),
            template(
                "doc_temp_report",
                "doc_temp_report",
                "Analytics Report",
                "analytics-report",
                true,
            ),
        ]
    }

    fn picks(kind: &str, family: &str) -> DesignSettings {
        DesignSettings {
            selections: Selections::from([(kind.to_string(), family.to_string())]),
            options: OptionValues::new(),
        }
    }

    #[test]
    fn no_choice_means_the_built_in_default() {
        let resolved =
            resolve_in(&catalog(), "a4-invoice", None, &DesignSettings::default()).unwrap();
        assert_eq!(resolved.info.name, "doc_temp_classic");
        assert!(resolved.is_default);
    }

    #[test]
    fn the_shops_family_is_used_and_is_not_the_default() {
        let resolved = resolve_in(
            &catalog(),
            "a4-invoice",
            None,
            &picks("a4-invoice", "fam_client"),
        )
        .unwrap();
        assert_eq!(resolved.info.name, "doc_temp_client2");
        assert!(!resolved.is_default);
    }

    #[test]
    fn an_explicit_request_beats_the_shops_choice() {
        let resolved = resolve_in(
            &catalog(),
            "a4-invoice",
            Some("doc_temp_modern"),
            &picks("a4-invoice", "fam_client"),
        )
        .unwrap();
        assert_eq!(resolved.info.name, "doc_temp_modern");
    }

    #[test]
    fn an_unavailable_choice_or_request_is_refused_not_replaced() {
        for (requested, settings) in [
            (Some("doc_temp_gone"), DesignSettings::default()),
            (None, picks("a4-invoice", "fam_revoked")),
            // A family of another kind is not a design for this one.
            (Some("doc_temp_receipt"), DesignSettings::default()),
        ] {
            let err = resolve_in(&catalog(), "a4-invoice", requested, &settings)
                .expect_err("must be refused");
            let (status, code, _) = err.status_code_code_and_message();
            assert_eq!(status.as_u16(), 422);
            assert_eq!(code, "TEMPLATE_UNAVAILABLE");
        }
    }

    #[test]
    fn a_kind_without_a_published_default_is_a_server_fault() {
        let only_modern = vec![catalog().remove(0)];
        assert!(resolve_in(&only_modern, "a4-invoice", None, &DesignSettings::default()).is_err());
    }

    #[test]
    fn cache_key_is_historic_for_the_default_and_per_template_otherwise() {
        let default =
            resolve_in(&catalog(), "a4-invoice", None, &DesignSettings::default()).unwrap();
        assert_eq!(cache_key("a4-invoice", &default), "a4-invoice");
        let other = resolve_in(
            &catalog(),
            "a4-invoice",
            Some("doc_temp_modern"),
            &DesignSettings::default(),
        )
        .unwrap();
        assert_eq!(
            cache_key("a4-invoice", &other),
            "a4-invoice--doc_temp_modern"
        );
    }

    #[test]
    fn listing_has_the_default_first_and_marks_an_unavailable_choice() {
        let settings = picks("a4-invoice", "fam_revoked");
        let listing = list_in(&catalog(), &settings).unwrap();
        let a4 = &listing[0];
        assert_eq!(a4.kind, "a4-invoice");
        assert_eq!(a4.families[0].family, "doc_temp_classic");
        assert!(a4.families[0].is_default);
        assert_eq!(
            a4.families
                .iter()
                .map(|f| f.title.as_str())
                .collect::<Vec<_>>(),
            ["A4 Invoice", "A4 Invoice Modern", "Client Invoice"]
        );
        assert_eq!(a4.selected.as_deref(), Some("fam_revoked"));
        assert_eq!(
            a4.effective, None,
            "the stored family is gone: nothing is in force"
        );
        // The other kinds are unaffected and report their default.
        assert_eq!(listing[1].effective.as_deref(), Some("doc_temp_receipt"));
    }

    #[test]
    fn option_values_default_to_the_designs_own_defaults_and_overlay_saved_ones() {
        let saved: OptionValues = OptionValues::from([(
            "fam_client".to_string(),
            HashMap::from([("showSign".to_string(), json!(false))]),
        )]);
        let settings = DesignSettings {
            options: saved,
            ..Default::default()
        };
        let listing = list_in(&catalog(), &settings).unwrap();
        let values = &listing[0].option_values["fam_client"];
        assert_eq!(values["showSign"], json!(false));
        assert_eq!(values["accent"], json!("blue"));
        assert!(!listing[0].option_values.contains_key("doc_temp_classic"));
    }

    #[test]
    fn with_options_adds_the_object_only_for_designs_that_declare_options() {
        let plain = resolve_in(&catalog(), "a4-invoice", None, &DesignSettings::default()).unwrap();
        let data = with_options(json!({"a": 1}), &plain).unwrap();
        assert_eq!(data, json!({"a": 1}));

        let client = resolve_in(
            &catalog(),
            "a4-invoice",
            Some("fam_client"),
            &DesignSettings::default(),
        )
        .unwrap();
        let data = with_options(json!({"a": 1}), &client).unwrap();
        assert_eq!(data["options"], json!({"showSign": true, "accent": "blue"}));
    }

    #[test]
    fn changing_an_option_changes_the_cache_key() {
        let key_for = |accent: &str| {
            let settings = DesignSettings {
                selections: Selections::new(),
                options: OptionValues::from([(
                    "fam_client".to_string(),
                    HashMap::from([("accent".to_string(), json!(accent))]),
                )]),
            };
            let resolved =
                resolve_in(&catalog(), "a4-invoice", Some("fam_client"), &settings).unwrap();
            cache_key("a4-invoice", &resolved)
        };
        assert_ne!(key_for("blue"), key_for("red"));
        assert_eq!(key_for("red"), key_for("red"));
        assert!(key_for("red").starts_with("a4-invoice--doc_temp_client2--"));
    }

    #[test]
    fn a_saved_value_that_no_longer_fits_the_design_is_refused() {
        let settings = DesignSettings {
            selections: Selections::new(),
            options: OptionValues::from([(
                "fam_client".to_string(),
                HashMap::from([("accent".to_string(), json!("green"))]),
            )]),
        };
        let err = resolve_in(&catalog(), "a4-invoice", Some("fam_client"), &settings)
            .expect_err("refused");
        assert_eq!(
            err.status_code_code_and_message().1,
            "TEMPLATE_OPTIONS_INVALID"
        );
    }

    #[test]
    fn merging_validates_selections_and_options_and_changes_nothing_on_failure() {
        let update = HashMap::from([("a4-invoice".to_string(), Some("fam_client".to_string()))]);
        let options = OptionValues::from([(
            "fam_client".to_string(),
            HashMap::from([("accent".to_string(), json!("red"))]),
        )]);
        let merged =
            merge_settings_in(&catalog(), DesignSettings::default(), &update, &options).unwrap();
        assert_eq!(merged.selections["a4-invoice"], "fam_client");
        assert_eq!(merged.options["fam_client"]["accent"], json!("red"));

        // Back to the standard design.
        let cleared = HashMap::from([("a4-invoice".to_string(), None)]);
        let after = merge_settings_in(&catalog(), merged, &cleared, &OptionValues::new()).unwrap();
        assert!(after.selections.is_empty());

        let bad_options = [
            json!("purple"), // not a listed choice
            json!(true),     // wrong type
        ];
        for value in bad_options {
            let options = OptionValues::from([(
                "fam_client".to_string(),
                HashMap::from([("accent".to_string(), value)]),
            )]);
            let err = merge_settings_in(
                &catalog(),
                DesignSettings::default(),
                &HashMap::new(),
                &options,
            )
            .expect_err("refused");
            assert_eq!(
                err.status_code_code_and_message().1,
                "TEMPLATE_OPTIONS_INVALID"
            );
        }
        let unknown_key = OptionValues::from([(
            "fam_client".to_string(),
            HashMap::from([("nope".to_string(), json!(1))]),
        )]);
        assert!(
            merge_settings_in(
                &catalog(),
                DesignSettings::default(),
                &HashMap::new(),
                &unknown_key
            )
            .is_err()
        );
        let unknown_family = OptionValues::from([("fam_none".to_string(), HashMap::new())]);
        assert!(
            merge_settings_in(
                &catalog(),
                DesignSettings::default(),
                &HashMap::new(),
                &unknown_family
            )
            .is_err()
        );
        let unknown_kind = HashMap::from([("poster".to_string(), None)]);
        assert!(
            merge_settings_in(
                &catalog(),
                DesignSettings::default(),
                &unknown_kind,
                &OptionValues::new()
            )
            .is_err()
        );
        // A family offered for a different kind cannot be picked here.
        let wrong_kind = HashMap::from([(
            "a4-invoice".to_string(),
            Some("doc_temp_receipt".to_string()),
        )]);
        assert!(
            merge_settings_in(
                &catalog(),
                DesignSettings::default(),
                &wrong_kind,
                &OptionValues::new()
            )
            .is_err()
        );
    }

    #[test]
    fn text_and_colour_options_are_checked() {
        let colour = DesignOption {
            key: "accent".into(),
            r#type: DesignOptionType::Color,
            label: "Accent".into(),
            default: json!("#112233"),
            choices: None,
            max_length: None,
        };
        assert!(validate_option_value(&colour, &json!("#AABBCC")).is_ok());
        assert!(validate_option_value(&colour, &json!("blue")).is_err());
        let text = DesignOption {
            key: "note".into(),
            r#type: DesignOptionType::Text,
            label: "Note".into(),
            default: json!(""),
            choices: None,
            max_length: Some(5),
        };
        assert!(validate_option_value(&text, &json!("hello")).is_ok());
        assert!(validate_option_value(&text, &json!("hello!")).is_err());
        assert!(validate_option_value(&text, &json!(7)).is_err());
    }
}
