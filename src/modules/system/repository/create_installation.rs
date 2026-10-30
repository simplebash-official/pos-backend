use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::system::SystemInstallation;
use mongodb::bson::{Document, doc};

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
