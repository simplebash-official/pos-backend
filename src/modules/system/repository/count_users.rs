use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use mongodb::bson::{Document, doc};
use sqlx::Row;

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
