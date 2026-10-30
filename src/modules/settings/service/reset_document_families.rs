use super::document_template_settings_key::DOCUMENT_TEMPLATE_SETTINGS_KEY;
use super::persist_document_template_settings::persist_document_template_settings;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::documents::service::templates::DesignSettings;

use crate::modules::settings::repository;

/// Removes every stored selection of the given families and the option
/// values saved for them; returns the `(kind, family)` selections removed.
/// An explicit operator action: printing then needs a new choice (or falls
/// to the standard design when the kind is left unchosen).
pub async fn reset_document_families(
    db: &Db,
    families: &[String],
) -> AppResult<Vec<(String, String)>> {
    crate::core::logging::domain::tracked("settings.document_designs_reset", async move {
        let existing =
            repository::find_document_template_settings(db, DOCUMENT_TEMPLATE_SETTINGS_KEY).await?;
        let Some(existing) = existing else {
            return Ok(Vec::new());
        };
        let mut settings = DesignSettings {
            selections: existing.selections.clone(),
            options: existing.options.clone(),
        };
        let mut removed: Vec<(String, String)> = settings
            .selections
            .iter()
            .filter(|(_, family)| families.contains(family))
            .map(|(kind, family)| (kind.clone(), family.clone()))
            .collect();
        removed.sort();
        settings
            .selections
            .retain(|_, family| !families.contains(family));
        settings
            .options
            .retain(|family, _| !families.contains(family));
        persist_document_template_settings(db, Some(existing), &settings).await?;
        Ok(removed)
    })
    .await
}
