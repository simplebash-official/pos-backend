use super::categories::categories;
use super::find_category_by_key::find_category_by_key;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::CategoryDocument;

use mongodb::bson::doc;

pub(crate) async fn delete_category(db: &Db, key: &str) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let deleted = categories(mongo)
                .find_one_and_delete(doc! { "key": key })
                .await?;
            if deleted.is_some() {
                crate::modules::sync::service::record_hard_deletes(
                    db,
                    "categories",
                    &[key.to_string()],
                )
                .await?;
            }
            Ok(deleted)
        }
        Db::Sqlite(pool) => {
            let existing = find_category_by_key(db, key).await?;
            if existing.is_some() {
                sqlx::query("DELETE FROM categories WHERE key = ?")
                    .bind(key)
                    .execute(pool)
                    .await?;
            }
            Ok(existing)
        }
    }
}
