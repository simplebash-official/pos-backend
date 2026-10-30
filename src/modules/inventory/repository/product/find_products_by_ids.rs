use super::products::products;
use super::row_to_product_doc::row_to_product_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_products_by_ids(
    db: &Db,
    ids: &[ObjectId],
) -> AppResult<Vec<ProductDocument>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = products(db)
                .find(doc! { "_id": { "$in": ids }, "deleted_at": { "$exists": false } })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders: Vec<_> = ids.iter().map(|_| "?").collect();
            let query_str = format!(
                "SELECT * FROM products WHERE id IN ({}) AND deleted_at IS NULL",
                placeholders.join(",")
            );
            let mut q = sqlx::query(&query_str);
            for id in ids {
                q = q.bind(id.to_hex());
            }
            let rows = q.fetch_all(pool).await?;
            rows.iter().map(row_to_product_doc).collect()
        }
    }
}
