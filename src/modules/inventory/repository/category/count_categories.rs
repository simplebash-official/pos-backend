use super::categories::categories;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

/// For `GET /inventory/stats`. Categories are hard-deleted (`delete_category`
/// below), not soft-deleted, so a plain count is the live total.
pub(crate) async fn count_categories(db: &Db) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(categories(db).count_documents(doc! {}).await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
                .fetch_one(pool)
                .await?;
            Ok(count as u64)
        }
    }
}
