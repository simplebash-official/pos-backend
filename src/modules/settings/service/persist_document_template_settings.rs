use super::document_template_settings_key::DOCUMENT_TEMPLATE_SETTINGS_KEY;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::documents::service::templates::DesignSettings;
use crate::modules::settings::model::DocumentTemplateSettingsDocument;
use crate::modules::settings::repository;

use mongodb::bson::DateTime as BsonDateTime;

pub(super) async fn persist_document_template_settings(
    db: &Db,
    existing: Option<DocumentTemplateSettingsDocument>,
    settings: &DesignSettings,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    let doc = match existing {
        Some(existing) => DocumentTemplateSettingsDocument {
            selections: settings.selections.clone(),
            options: settings.options.clone(),
            version: existing.version + 1,
            updated_at: now,
            ..existing
        },
        None => DocumentTemplateSettingsDocument {
            id: None,
            key: DOCUMENT_TEMPLATE_SETTINGS_KEY.to_string(),
            selections: settings.selections.clone(),
            options: settings.options.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
        },
    };
    repository::upsert_document_template_settings(db, &doc).await
}
