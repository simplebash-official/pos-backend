use super::product_serials::product_serials;
use super::row_to_serial_doc::row_to_serial_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductSerialDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub(crate) async fn find_serials_for_product(
    db: &Db,
    product_key: &str,
    status: Option<&str>,
) -> AppResult<Vec<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! { "product_key": product_key };
            if let Some(status) = status {
                filter.insert("status", status);
            }
            let mut cursor = product_serials(db)
                .find(filter)
                .sort(doc! { "serial_number": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = if let Some(st) = status {
                sqlx::query(
                    "SELECT * FROM product_serials WHERE product_key = ? AND status = ? ORDER BY serial_number ASC",
                )
                .bind(product_key)
                .bind(st)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT * FROM product_serials WHERE product_key = ? ORDER BY serial_number ASC",
                )
                .bind(product_key)
                .fetch_all(pool)
                .await?
            };

            rows.iter().map(row_to_serial_doc).collect()
        }
    }
}
