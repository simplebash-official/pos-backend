use super::row_to_subcategory_doc::row_to_subcategory_doc;
use super::subcategories::subcategories;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::SubcategoryDocument;
use futures_util::TryStreamExt;

use mongodb::bson::doc;

/// Sorted by name for stable, predictable output.
pub(crate) async fn list_subcategories_by_category(
    db: &Db,
    category_key: &str,
) -> AppResult<Vec<SubcategoryDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = subcategories(db)
                .find(doc! { "category_key": category_key })
                .sort(doc! { "name": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows =
                sqlx::query("SELECT * FROM subcategories WHERE category_key = ? ORDER BY name ASC")
                    .bind(category_key)
                    .fetch_all(pool)
                    .await?;

            rows.iter().map(row_to_subcategory_doc).collect()
        }
    }
}
