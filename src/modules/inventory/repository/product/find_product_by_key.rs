use super::products::products;
use super::row_to_product_doc::row_to_product_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::doc;

pub(crate) async fn find_product_by_key(db: &Db, key: &str) -> AppResult<Option<ProductDocument>> {
    match db {
        Db::Mongo(db) => Ok(products(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM products WHERE key = ? AND deleted_at IS NULL")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}
