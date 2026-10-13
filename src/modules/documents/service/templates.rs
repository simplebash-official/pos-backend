// Which document-server template prints a given document. The document-server
// publishes every template with the kind it is for; a shop stores one chosen
// template per kind, and a single print can override it. This file owns the
// rules (all pure functions over the template list, so they are unit-tested
// without HTTP) plus thin async wrappers that fetch the list.
//
// Resolution order at print time:
//   1. an explicit `?template=` on the request  -> must be available, else 422
//   2. the shop's stored choice for that kind   -> quietly falls back if the
//      template has since been removed (the shop must still be able to print)
//   3. the kind's built-in default (`clients::document_server::default_title`)

use std::collections::HashMap;

use crate::{
    clients::document_server::{DocumentServerClient, TemplateInfo, default_title},
    core::error::{AppError, AppResult},
    modules::documents::kinds,
};

/// A shop's stored choices: document kind slug -> template `name`.
pub(crate) type Selections = HashMap<String, String>;

/// The template picked for one print, plus whether it is the built-in default.
pub(crate) struct ResolvedTemplate {
    /// The kind's slug (what `generated_documents.template_name` stores).
    pub kind: &'static str,
    pub info: TemplateInfo,
    pub is_default: bool,
}

/// One entry in a kind's dropdown.
pub(crate) struct TemplateOption {
    pub name: String,
    pub title: String,
    pub is_default: bool,
}

/// Everything the Settings screen needs for one kind.
pub(crate) struct KindListing {
    pub kind: &'static str,
    pub label: &'static str,
    /// The shop's stored choice, only while that template is still available.
    pub selected: Option<String>,
    /// The template prints use today (`selected`, else the default).
    pub effective: String,
    /// Default first, then by title.
    pub templates: Vec<TemplateOption>,
}

fn unavailable(name: &str) -> AppError {
    AppError::unprocessable_entity(
        "TEMPLATE_UNAVAILABLE",
        format!(
            "The design '{name}' is not available for this document. \
             Choose another design in Settings."
        ),
    )
}

/// The kind's built-in default template — found by its title, and not
/// required to carry the kind marker (a document-server older than the
/// marker still serves its defaults).
fn default_for<'a>(templates: &'a [TemplateInfo], kind: &str) -> AppResult<&'a TemplateInfo> {
    let title = default_title(kind)
        .ok_or_else(|| AppError::internal(format!("no built-in template for kind '{kind}'")))?;
    templates
        .iter()
        .find(|template| template.title == title)
        .ok_or_else(|| {
            AppError::internal(format!(
                "document-server has no template described as '{title}' (document kind '{kind}')"
            ))
        })
}

/// The active template `name`, if it may print this kind: either it declares
/// the kind or it is the kind's built-in default.
fn find_available<'a>(
    templates: &'a [TemplateInfo],
    default: &TemplateInfo,
    kind: &str,
    name: &str,
) -> Option<&'a TemplateInfo> {
    templates.iter().find(|template| {
        template.name == name
            && (template.name == default.name || template.kind.as_deref() == Some(kind))
    })
}

pub(crate) fn resolve_in(
    templates: &[TemplateInfo],
    kind: &str,
    requested: Option<&str>,
    selections: &Selections,
) -> AppResult<ResolvedTemplate> {
    let kind_def = kinds::find(kind)
        .ok_or_else(|| AppError::internal(format!("unknown document kind '{kind}'")))?;
    let default = default_for(templates, kind)?;

    let chosen = if let Some(name) = requested {
        find_available(templates, default, kind, name).ok_or_else(|| unavailable(name))?
    } else if let Some(name) = selections.get(kind) {
        find_available(templates, default, kind, name).unwrap_or_else(|| {
            tracing::warn!(
                kind,
                template = %name,
                "the shop's chosen template is no longer available; using the standard design"
            );
            default
        })
    } else {
        default
    };

    Ok(ResolvedTemplate {
        kind: kind_def.slug,
        is_default: chosen.name == default.name,
        info: chosen.clone(),
    })
}

/// The `generated_documents` cache key for a print. The built-in default keeps
/// the kind's historical key, so every PDF cached before templates were
/// selectable stays valid; any other template gets its own key, so a design is
/// never served from another design's cache.
pub(crate) fn cache_key(legacy_key: &str, resolved: &ResolvedTemplate) -> String {
    if resolved.is_default {
        legacy_key.to_string()
    } else {
        format!("{legacy_key}--{}", resolved.info.name)
    }
}

/// What the Settings screen offers per kind. A kind whose built-in default is
/// not published (a document-server older than this backend — e.g. one that
/// predates the Quotation template) is left out, with a warning, rather than
/// failing the whole listing: one missing optional design must never take the
/// invoice/receipt design pickers down with it. Printing that kind still
/// fails loudly in `resolve_in`, which is where the gap actually matters.
pub(crate) fn list_in(
    templates: &[TemplateInfo],
    selections: &Selections,
) -> AppResult<Vec<KindListing>> {
    let listings = kinds::SELECTABLE_KINDS
        .iter()
        .filter_map(|kind| {
            let default = match default_for(templates, kind.slug) {
                Ok(default) => default,
                Err(err) => {
                    tracing::warn!(kind = kind.slug, error = %err, "document kind left out of the design listing");
                    return None;
                }
            };
            let mut options: Vec<TemplateOption> = templates
                .iter()
                .filter(|template| {
                    template.name == default.name || template.kind.as_deref() == Some(kind.slug)
                })
                .map(|template| TemplateOption {
                    name: template.name.clone(),
                    title: template.title.clone(),
                    is_default: template.name == default.name,
                })
                .collect();
            options.sort_by(|a, b| {
                b.is_default
                    .cmp(&a.is_default)
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                    .then_with(|| a.name.cmp(&b.name))
            });

            let selected = selections
                .get(kind.slug)
                .filter(|name| options.iter().any(|option| &option.name == *name))
                .cloned();
            let effective = selected.clone().unwrap_or_else(|| default.name.clone());
            Some(KindListing {
                kind: kind.slug,
                label: kind.label,
                selected,
                effective,
                templates: options,
            })
        })
        .collect();
    Ok(listings)
}

/// Applies an admin's update (`None` = back to the default) to the stored
/// choices, validating every pick. Nothing is applied unless all of it is valid.
pub(crate) fn merge_selections_in(
    templates: &[TemplateInfo],
    mut current: Selections,
    update: &HashMap<String, Option<String>>,
) -> AppResult<Selections> {
    for (kind, choice) in update {
        let kind_def = kinds::find(kind)
            .ok_or_else(|| AppError::validation(format!("Unknown document kind '{kind}'")))?;
        match choice {
            None => {
                current.remove(kind_def.slug);
            }
            Some(name) => {
                let default = default_for(templates, kind_def.slug)?;
                find_available(templates, default, kind_def.slug, name)
                    .ok_or_else(|| unavailable(name))?;
                current.insert(kind_def.slug.to_string(), name.clone());
            }
        }
    }
    Ok(current)
}

pub(crate) async fn resolve(
    client: &DocumentServerClient,
    kind: &str,
    requested: Option<&str>,
    selections: &Selections,
) -> AppResult<ResolvedTemplate> {
    let templates = client.list_templates().await?;
    resolve_in(&templates, kind, requested, selections)
}

pub(crate) async fn list(
    client: &DocumentServerClient,
    selections: &Selections,
) -> AppResult<Vec<KindListing>> {
    let templates = client.list_templates().await?;
    list_in(&templates, selections)
}

pub(crate) async fn merge_selections(
    client: &DocumentServerClient,
    current: Selections,
    update: &HashMap<String, Option<String>>,
) -> AppResult<Selections> {
    let templates = client.list_templates().await?;
    merge_selections_in(&templates, current, update)
}

/// PDF of a template's own sample data, for the Settings preview. Only
/// templates a shop can actually choose (or a kind's default) are previewable.
pub(crate) async fn render_preview(
    client: &DocumentServerClient,
    name: &str,
) -> AppResult<Vec<u8>> {
    let templates = client.list_templates().await?;
    let previewable = |template: &&TemplateInfo| {
        template.name == name
            && kinds::SELECTABLE_KINDS.iter().any(|kind| {
                template.kind.as_deref() == Some(kind.slug)
                    || default_title(kind.slug) == Some(template.title.as_str())
            })
    };
    let info = templates.iter().find(previewable).ok_or_else(|| {
        AppError::not_found_with_code(
            "That design is not available.".to_string(),
            "TEMPLATE_UNAVAILABLE",
        )
    })?;
    client.render_sample(info).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(name: &str, title: &str, kind: Option<&str>) -> TemplateInfo {
        TemplateInfo {
            key: format!("tpl_{name}"),
            name: name.to_string(),
            title: title.to_string(),
            kind: kind.map(str::to_string),
            data_schema: None,
            sample: None,
        }
    }

    fn catalog() -> Vec<TemplateInfo> {
        vec![
            template("doc_temp_modern", "A4 Invoice Modern", Some("a4-invoice")),
            template("doc_temp_classic", "A4 Invoice", Some("a4-invoice")),
            template(
                "doc_temp_receipt",
                "Thermal Receipt",
                Some("thermal-receipt"),
            ),
            template("doc_temp_credit", "Credit Note", Some("credit-note")),
            template("doc_temp_quote", "Quotation", Some("quotation")),
            template(
                "doc_temp_report",
                "Analytics Report",
                Some("analytics-report"),
            ),
        ]
    }

    fn picks(kind: &str, name: &str) -> Selections {
        Selections::from([(kind.to_string(), name.to_string())])
    }

    #[test]
    fn no_choice_means_the_built_in_default() {
        let resolved = resolve_in(&catalog(), "a4-invoice", None, &Selections::new()).unwrap();
        assert_eq!(resolved.info.name, "doc_temp_classic");
        assert!(resolved.is_default);
        assert_eq!(resolved.kind, "a4-invoice");
    }

    #[test]
    fn the_shops_choice_is_used() {
        let resolved = resolve_in(
            &catalog(),
            "a4-invoice",
            None,
            &picks("a4-invoice", "doc_temp_modern"),
        )
        .unwrap();
        assert_eq!(resolved.info.name, "doc_temp_modern");
        assert!(!resolved.is_default);
    }

    #[test]
    fn an_explicit_request_beats_the_shops_choice() {
        let resolved = resolve_in(
            &catalog(),
            "a4-invoice",
            Some("doc_temp_classic"),
            &picks("a4-invoice", "doc_temp_modern"),
        )
        .unwrap();
        assert_eq!(resolved.info.name, "doc_temp_classic");
        assert!(resolved.is_default);
    }

    #[test]
    fn a_vanished_shop_choice_quietly_falls_back_to_the_default() {
        let resolved = resolve_in(
            &catalog(),
            "a4-invoice",
            None,
            &picks("a4-invoice", "doc_temp_gone"),
        )
        .unwrap();
        assert_eq!(resolved.info.name, "doc_temp_classic");
    }

    #[test]
    fn an_unavailable_explicit_request_is_a_422() {
        for bad in ["doc_temp_gone", "doc_temp_receipt"] {
            let err = resolve_in(&catalog(), "a4-invoice", Some(bad), &Selections::new())
                .err()
                .unwrap();
            assert!(
                format!("{err:?}").contains("TEMPLATE_UNAVAILABLE"),
                "{bad}: {err:?}"
            );
        }
    }

    #[test]
    fn the_default_is_found_even_when_it_has_no_kind_marker() {
        let mut templates = catalog();
        templates[1].kind = None;
        let resolved = resolve_in(
            &templates,
            "a4-invoice",
            Some("doc_temp_classic"),
            &Selections::new(),
        )
        .unwrap();
        assert_eq!(resolved.info.name, "doc_temp_classic");
    }

    #[test]
    fn only_the_default_keeps_the_legacy_cache_key() {
        let default = resolve_in(&catalog(), "thermal-receipt", None, &Selections::new()).unwrap();
        assert_eq!(
            cache_key("thermal-receipt-80mm", &default),
            "thermal-receipt-80mm"
        );

        let modern = resolve_in(
            &catalog(),
            "a4-invoice",
            Some("doc_temp_modern"),
            &Selections::new(),
        )
        .unwrap();
        assert_eq!(
            cache_key("a4-invoice", &modern),
            "a4-invoice--doc_temp_modern"
        );
    }

    #[test]
    fn listing_offers_only_the_kinds_templates_default_first_then_by_title() {
        let listing = list_in(&catalog(), &picks("a4-invoice", "doc_temp_modern")).unwrap();
        assert_eq!(
            listing.iter().map(|k| k.kind).collect::<Vec<_>>(),
            ["a4-invoice", "thermal-receipt", "credit-note", "quotation"]
        );
        let invoice = &listing[0];
        assert_eq!(
            invoice
                .templates
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["doc_temp_classic", "doc_temp_modern"]
        );
        assert!(invoice.templates[0].is_default);
        assert_eq!(invoice.selected.as_deref(), Some("doc_temp_modern"));
        assert_eq!(invoice.effective, "doc_temp_modern");

        let receipt = &listing[1];
        assert_eq!(receipt.templates.len(), 1);
        assert_eq!(receipt.selected, None);
        assert_eq!(receipt.effective, "doc_temp_receipt");
    }

    #[test]
    fn a_kind_whose_default_is_not_published_is_left_out_not_an_error() {
        // A document-server that predates the Quotation template.
        let older: Vec<TemplateInfo> = catalog()
            .into_iter()
            .filter(|t| t.title != "Quotation")
            .collect();
        let listing = list_in(&older, &Selections::new()).unwrap();
        assert_eq!(
            listing.iter().map(|k| k.kind).collect::<Vec<_>>(),
            ["a4-invoice", "thermal-receipt", "credit-note"]
        );
        // Printing that kind is where the gap is reported.
        let err = resolve_in(&older, "quotation", None, &Selections::new())
            .err()
            .unwrap();
        assert!(format!("{err:?}").contains("Quotation"), "{err:?}");
    }

    #[test]
    fn a_stale_stored_choice_is_reported_as_not_selected() {
        let listing = list_in(&catalog(), &picks("a4-invoice", "doc_temp_gone")).unwrap();
        assert_eq!(listing[0].selected, None);
        assert_eq!(listing[0].effective, "doc_temp_classic");
    }

    #[test]
    fn updates_are_validated_and_null_clears_a_choice() {
        let update = HashMap::from([(
            "a4-invoice".to_string(),
            Some("doc_temp_modern".to_string()),
        )]);
        let merged = merge_selections_in(&catalog(), Selections::new(), &update).unwrap();
        assert_eq!(
            merged.get("a4-invoice").map(String::as_str),
            Some("doc_temp_modern")
        );

        let clear = HashMap::from([("a4-invoice".to_string(), None)]);
        let cleared = merge_selections_in(&catalog(), merged, &clear).unwrap();
        assert!(cleared.is_empty());

        for bad in [
            HashMap::from([(
                "a4-invoice".to_string(),
                Some("doc_temp_receipt".to_string()),
            )]),
            HashMap::from([("a4-invoice".to_string(), Some("doc_temp_gone".to_string()))]),
            HashMap::from([("nonsense".to_string(), Some("doc_temp_modern".to_string()))]),
        ] {
            assert!(merge_selections_in(&catalog(), Selections::new(), &bad).is_err());
        }
    }

    #[test]
    fn a_failed_update_changes_nothing() {
        let current = picks("credit-note", "doc_temp_credit");
        let update = HashMap::from([
            (
                "a4-invoice".to_string(),
                Some("doc_temp_modern".to_string()),
            ),
            (
                "thermal-receipt".to_string(),
                Some("doc_temp_gone".to_string()),
            ),
        ]);
        assert!(merge_selections_in(&catalog(), current.clone(), &update).is_err());
        assert_eq!(current.len(), 1);
    }
}
