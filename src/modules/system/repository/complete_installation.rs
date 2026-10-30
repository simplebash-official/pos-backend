use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use chrono::Utc;
use mongodb::bson::{Document, doc};

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
