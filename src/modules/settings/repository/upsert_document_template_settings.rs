use super::document_template_settings::DOCUMENT_TEMPLATE_SETTINGS;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::{AppError, AppResult};
use crate::modules::settings::model::DocumentTemplateSettingsDocument;
use mongodb::bson::doc;

pub(crate) async fn upsert_document_template_settings(
    db: &Db,
    doc: &DocumentTemplateSettingsDocument,
) -> AppResult<()> {
    let save_err = |e: &dyn std::fmt::Display| {
        AppError::internal(format!("Failed to save document template settings: {e}"))
    };
    match db {
        Db::Sqlite(pool) => {
            let selections_json =
                serde_json::to_string(&doc.selections).map_err(|e| save_err(&e))?;
            let options_json = serde_json::to_string(&doc.options).map_err(|e| save_err(&e))?;
            sqlx::query(
                "INSERT INTO document_template_settings (key, selections, options, version, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(key) DO UPDATE SET \
                    selections = excluded.selections, \
                    options = excluded.options, \
                    version = excluded.version, \
                    updated_at = excluded.updated_at",
            )
            .bind(&doc.key)
            .bind(&selections_json)
            .bind(&options_json)
            .bind(doc.version)
            .bind(bson_to_iso(&doc.created_at))
            .bind(bson_to_iso(&doc.updated_at))
            .execute(pool)
            .await
            .map_err(|e| save_err(&e))?;
            Ok(())
        }
        Db::Mongo(mongo) => {
            let selections =
                mongodb::bson::serialize_to_bson(&doc.selections).map_err(|e| save_err(&e))?;
            let options =
                mongodb::bson::serialize_to_bson(&doc.options).map_err(|e| save_err(&e))?;
            mongo
                .collection::<mongodb::bson::Document>(DOCUMENT_TEMPLATE_SETTINGS)
                .update_one(
                    doc! { "key": &doc.key },
                    doc! {
                        "$set": {
                            "key": &doc.key,
                            "selections": selections,
                            "options": options,
                            "version": doc.version,
                            "updated_at": doc.updated_at,
                        },
                        "$setOnInsert": { "created_at": doc.created_at },
                    },
                )
                .upsert(true)
                .await
                .map_err(|e| save_err(&e))?;
            Ok(())
        }
    }
}
