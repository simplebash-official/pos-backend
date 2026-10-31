use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::system::SystemInstallation;
use chrono::Utc;
use mongodb::bson::{Document, doc};
use sqlx::Row;

/// Marks the installation setup as completed with the chosen sample data flag.
pub(crate) async fn complete_installation(
    db: &Db,
    installation_id: &str,
    sample_data_loaded: bool,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    match db {
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE system_installations \
                 SET setup_completed = 1, sample_data_loaded = ?1, setup_completed_at = ?2, updated_at = ?3 \
                 WHERE installation_id = ?4",
            )
            .bind(if sample_data_loaded { 1i64 } else { 0i64 })
            .bind(&now)
            .bind(&now)
            .bind(installation_id)
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to complete system installation: {e}")))?;

            Ok(())
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<Document>("system_installations");
            collection
                .update_one(
                    doc! { "installation_id": installation_id },
                    doc! {
                        "$set": {
                            "setup_completed": true,
                            "sample_data_loaded": sample_data_loaded,
                            "setup_completed_at": &now,
                            "updated_at": &now,
                        }
                    },
                )
                .await
                .map_err(|e| {
                    AppError::internal(format!("Failed to update system installation: {e}"))
                })?;

            Ok(())
        }
    }
}

/// Counts total active non-deleted users in the system.
pub(crate) async fn count_users(db: &Db) -> AppResult<i64> {
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM users WHERE deleted_at IS NULL")
                .fetch_one(pool)
                .await
                .map_err(|e| AppError::internal(format!("Failed to count users: {e}")))?;

            let count: i64 = row.get("count");
            Ok(count)
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<Document>("users");
            let count = collection
                .count_documents(doc! { "deleted_at": null })
                .await
                .map_err(|e| AppError::internal(format!("Failed to count users: {e}")))?;

            Ok(count as i64)
        }
    }
}

/// Creates a new system installation record.
pub(crate) async fn create_installation(db: &Db, record: &SystemInstallation) -> AppResult<()> {
    match db {
        Db::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO system_installations \
                 (key, id, installation_id, app_version, platform, installed_at, setup_completed, setup_completed_at, sample_data_loaded, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .bind(&record.key)
            .bind(&record.id)
            .bind(&record.installation_id)
            .bind(&record.app_version)
            .bind(&record.platform)
            .bind(&record.installed_at)
            .bind(if record.setup_completed { 1i64 } else { 0i64 })
            .bind(&record.setup_completed_at)
            .bind(if record.sample_data_loaded { 1i64 } else { 0i64 })
            .bind(&record.created_at)
            .bind(&record.updated_at)
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to record system installation: {e}")))?;

            Ok(())
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<Document>("system_installations");
            let mut doc = doc! {
                "key": &record.key,
                "id": &record.id,
                "installation_id": &record.installation_id,
                "app_version": &record.app_version,
                "platform": &record.platform,
                "installed_at": &record.installed_at,
                "setup_completed": record.setup_completed,
                "sample_data_loaded": record.sample_data_loaded,
                "created_at": &record.created_at,
                "updated_at": &record.updated_at,
            };
            if let Some(ref completed_at) = record.setup_completed_at {
                doc.insert("setup_completed_at", completed_at);
            }
            collection.insert_one(doc).await.map_err(|e| {
                AppError::internal(format!("Failed to insert system installation: {e}"))
            })?;

            Ok(())
        }
    }
}

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
                    key: text(&doc, "key")?,
                    id: text(&doc, "id")?,
                    installation_id: text(&doc, "installation_id")?,
                    app_version: text(&doc, "app_version")?,
                    platform: text(&doc, "platform")?,
                    installed_at: text(&doc, "installed_at")?,
                    setup_completed: flag(&doc, "setup_completed")?,
                    setup_completed_at: doc.get_str("setup_completed_at").ok().map(String::from),
                    sample_data_loaded: flag(&doc, "sample_data_loaded")?,
                    created_at: text(&doc, "created_at")?,
                    updated_at: text(&doc, "updated_at")?,
                }))
            } else {
                Ok(None)
            }
        }
    }
}

/// A required string field of a stored installation document.
fn text(doc: &mongodb::bson::Document, field: &str) -> AppResult<String> {
    doc.get_str(field)
        .map(str::to_string)
        .map_err(|err| AppError::internal(format!("installation has no `{field}`: {err}")))
}

/// A required boolean field of a stored installation document.
fn flag(doc: &mongodb::bson::Document, field: &str) -> AppResult<bool> {
    doc.get_bool(field)
        .map_err(|err| AppError::internal(format!("installation has no `{field}`: {err}")))
}
