use super::delete_links_where::delete_links_where;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn delete_links_by_product(db: &Db, product_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(mongo) => {
            delete_links_where(db, mongo, doc! { "product_key": product_key }).await
        }
        Db::Sqlite(pool) => {
            let res = sqlx::query("DELETE FROM supplier_products WHERE product_key = ?")
                .bind(product_key)
                .execute(pool)
                .await?;
            Ok(res.rows_affected())
        }
    }
}
