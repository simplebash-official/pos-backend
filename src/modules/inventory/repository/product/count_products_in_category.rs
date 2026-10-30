use super::products::products;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn count_products_in_category(db: &Db, category_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .count_documents(
                doc! { "category_key": category_key, "deleted_at": { "$exists": false } },
            )
            .await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE category_key = ? AND deleted_at IS NULL",
            )
            .bind(category_key)
            .fetch_one(pool)
            .await?;
            Ok(count as u64)
        }
    }
}
