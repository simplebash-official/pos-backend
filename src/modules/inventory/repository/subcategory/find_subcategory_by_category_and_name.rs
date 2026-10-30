use super::row_to_subcategory_doc::row_to_subcategory_doc;
use super::subcategories::subcategories;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::SubcategoryDocument;

use mongodb::bson::doc;

/// Backs the duplicate-name guard on `service::category::add_subcategory` —
/// names only need to be unique within a single category, not globally.
pub(crate) async fn find_subcategory_by_category_and_name(
    db: &Db,
    category_key: &str,
    name: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db)
            .find_one(doc! { "category_key": category_key, "name": name })
            .await?),
        Db::Sqlite(pool) => {
            let row =
                sqlx::query("SELECT * FROM subcategories WHERE category_key = ? AND name = ?")
                    .bind(category_key)
                    .bind(name)
                    .fetch_optional(pool)
                    .await?;

            row.map(|r| row_to_subcategory_doc(&r)).transpose()
        }
    }
}
