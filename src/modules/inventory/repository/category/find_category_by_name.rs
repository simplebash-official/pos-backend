use super::categories::categories;
use super::row_to_category_doc::row_to_category_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::CategoryDocument;

use mongodb::bson::doc;

/// Only used for the create-time name-uniqueness check
/// (`service::category::create_category`/`update_category`) — every other
/// lookup goes by `key` (see `find_category_by_key`).
pub(crate) async fn find_category_by_name(
    db: &Db,
    name: &str,
) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db).find_one(doc! { "name": name }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM categories WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_category_doc(&r)).transpose()
        }
    }
}
