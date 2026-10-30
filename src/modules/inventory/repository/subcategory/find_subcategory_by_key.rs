use super::row_to_subcategory_doc::row_to_subcategory_doc;
use super::subcategories::subcategories;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::SubcategoryDocument;

use mongodb::bson::doc;

pub(crate) async fn find_subcategory_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM subcategories WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_subcategory_doc(&r)).transpose()
        }
    }
}
