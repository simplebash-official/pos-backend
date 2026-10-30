use super::sessions_col::sessions_col;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub async fn count_active_sessions(db: &Db) -> AppResult<i64> {
    match db {
        Db::Mongo(mongo_db) => {
            let count = sessions_col(mongo_db)
                .count_documents(doc! { "status": { "$in": ["open", "closing", "reopened"] } })
                .await?;
            Ok(count as i64)
        }
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM register_sessions WHERE status IN ('open', 'closing', 'reopened') AND deleted_at IS NULL"
            )
            .fetch_one(pool)
            .await?;
            Ok(count)
        }
    }
}
