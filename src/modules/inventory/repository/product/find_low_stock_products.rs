use super::products::products;
use super::row_to_product_doc::row_to_product_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;
use futures_util::TryStreamExt;

use mongodb::bson::doc;

pub(crate) async fn find_low_stock_products(db: &Db) -> AppResult<Vec<ProductDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = products(db)
                .find(doc! {
                    "deleted_at": { "$exists": false },
                    "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
                })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM products WHERE deleted_at IS NULL AND stock_quantity <= min_stock_threshold",
            )
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_product_doc).collect()
        }
    }
}
