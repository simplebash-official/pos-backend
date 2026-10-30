use super::find_subcategory_by_key::find_subcategory_by_key;
use super::subcategories::subcategories;
use super::touch_parent_category::touch_parent_category;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::SubcategoryDocument;

use mongodb::bson::doc;

pub(crate) async fn delete_subcategory_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let deleted = subcategories(mongo)
                .find_one_and_delete(doc! { "key": key })
                .await?;
            if let Some(sub) = &deleted {
                touch_parent_category(mongo, &sub.category_key).await?;
            }
            Ok(deleted)
        }
        Db::Sqlite(pool) => {
            let existing = find_subcategory_by_key(db, key).await?;
            if existing.is_some() {
                sqlx::query("DELETE FROM subcategories WHERE key = ?")
                    .bind(key)
                    .execute(pool)
                    .await?;
            }
            Ok(existing)
        }
    }
}
