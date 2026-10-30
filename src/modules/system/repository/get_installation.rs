use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::system::SystemInstallation;
use mongodb::bson::{Document, doc};
use sqlx::Row;

/// Fetches the latest system installation record, if one exists.
pub(crate) async fn get_installation(db: &Db) -> AppResult<Option<SystemInstallation>> {
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT key, id, installation_id, app_version, platform, installed_at, \
                 setup_completed, setup_completed_at, sample_data_loaded, created_at, updated_at \
                 FROM system_installations ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to fetch system installation: {e}")))?;

            Ok(row.map(|r| {
                let setup_completed_int: i64 = r.get("setup_completed");
                let sample_data_loaded_int: i64 = r.get("sample_data_loaded");
                SystemInstallation {
                    key: r.get("key"),
                    id: r.get("id"),
                    installation_id: r.get("installation_id"),
                    app_version: r.get("app_version"),
                    platform: r.get("platform"),
                    installed_at: r.get("installed_at"),
                    setup_completed: setup_completed_int != 0,
                    setup_completed_at: r.get("setup_completed_at"),
                    sample_data_loaded: sample_data_loaded_int != 0,
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                }
            }))
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<Document>("system_installations");
            let doc_opt = collection
                .find_one(doc! {})
                .sort(doc! { "created_at": -1 })
                .await
                .map_err(|e| {
                    AppError::internal(format!("Failed to query system installations: {e}"))
                })?;

            if let Some(doc) = doc_opt {
                Ok(Some(SystemInstallation {
                    key: doc.get_str("key").unwrap_or_default().to_string(),
                    id: doc.get_str("id").unwrap_or_default().to_string(),
                    installation_id: doc
                        .get_str("installation_id")
                        .unwrap_or_default()
                        .to_string(),
                    app_version: doc.get_str("app_version").unwrap_or_default().to_string(),
                    platform: doc.get_str("platform").unwrap_or_default().to_string(),
                    installed_at: doc.get_str("installed_at").unwrap_or_default().to_string(),
                    setup_completed: doc.get_bool("setup_completed").unwrap_or(false),
                    setup_completed_at: doc.get_str("setup_completed_at").ok().map(String::from),
                    sample_data_loaded: doc.get_bool("sample_data_loaded").unwrap_or(false),
                    created_at: doc.get_str("created_at").unwrap_or_default().to_string(),
                    updated_at: doc.get_str("updated_at").unwrap_or_default().to_string(),
                }))
            } else {
                Ok(None)
            }
        }
    }
}
