use super::products::products;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn count_products_in_subcategory(
    db: &Db,
    subcategory_key: &str,
) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .count_documents(
                doc! { "subcategory_key": subcategory_key, "deleted_at": { "$exists": false } },
            )
            .await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE subcategory_key = ? AND deleted_at IS NULL",
            )
            .bind(subcategory_key)
            .fetch_one(pool)
            .await?;
            Ok(count as u64)
        }
    }
}
