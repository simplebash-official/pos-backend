use super::categories::categories;
use super::row_to_category_doc::row_to_category_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::CategoryDocument;

use mongodb::bson::doc;

pub(crate) async fn find_category_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM categories WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_category_doc(&r)).transpose()
        }
    }
}
