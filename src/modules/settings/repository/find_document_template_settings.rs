use super::document_template_settings::DOCUMENT_TEMPLATE_SETTINGS;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::modules::settings::model::DocumentTemplateSettingsDocument;
use mongodb::bson::doc;
use sqlx::Row;

pub(crate) async fn find_document_template_settings(
    db: &Db,
    key: &str,
) -> AppResult<Option<DocumentTemplateSettingsDocument>> {
    let fetch_err = |e: &dyn std::fmt::Display| {
        AppError::internal(format!("Failed to fetch document template settings: {e}"))
    };
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT key, selections, options, version, created_at, updated_at \
                 FROM document_template_settings WHERE key = ?1 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|e| fetch_err(&e))?;

            Ok(row.map(|r| {
                let selections_json: String = r.get("selections");
                let options_json: String = r.get("options");
                let created_at: String = r.get("created_at");
                let updated_at: String = r.get("updated_at");
                DocumentTemplateSettingsDocument {
                    id: None,
                    key: r.get("key"),
                    selections: serde_json::from_str(&selections_json).unwrap_or_default(),
                    options: serde_json::from_str(&options_json).unwrap_or_default(),
                    version: r.get("version"),
                    created_at: to_bson_datetime(&created_at),
                    updated_at: to_bson_datetime(&updated_at),
                }
            }))
        }
        Db::Mongo(mongo) => mongo
            .collection::<DocumentTemplateSettingsDocument>(DOCUMENT_TEMPLATE_SETTINGS)
            .find_one(doc! { "key": key })
            .await
            .map_err(|e| fetch_err(&e)),
    }
}
