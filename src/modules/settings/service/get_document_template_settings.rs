use super::document_template_settings_key::DOCUMENT_TEMPLATE_SETTINGS_KEY;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::documents::service::templates::DesignSettings;

use crate::modules::settings::repository;

/// The shop's stored designs: the family chosen per document kind and the
/// option values saved per design (both empty when the shop never changed
/// anything). Printing reads this to decide which design to render.
pub(crate) async fn get_document_template_settings(db: &Db) -> AppResult<DesignSettings> {
    Ok(
        repository::find_document_template_settings(db, DOCUMENT_TEMPLATE_SETTINGS_KEY)
            .await?
            .map(|doc| DesignSettings {
                selections: doc.selections,
                options: doc.options,
            })
            .unwrap_or_default(),
    )
}
