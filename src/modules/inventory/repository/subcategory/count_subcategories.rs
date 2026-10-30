use super::subcategories::subcategories;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

/// For `GET /inventory/stats`. Subcategories are hard-deleted, not
/// soft-deleted, so a plain count is the live total.
pub(crate) async fn count_subcategories(db: &Db) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db).count_documents(doc! {}).await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subcategories")
                .fetch_one(pool)
                .await?;
            Ok(count as u64)
        }
    }
}
