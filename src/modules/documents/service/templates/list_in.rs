use super::default_for::default_for;
use super::design_settings::DesignSettings;
use super::effective_options::effective_options;
use super::find_family::find_family;
use crate::clients::document_server::TemplateInfo;
use crate::core::error::AppResult;
use crate::domain::settings::{DocumentDesignFamily, DocumentKindSettings};
use crate::modules::documents::kinds;
use std::collections::HashMap;

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
