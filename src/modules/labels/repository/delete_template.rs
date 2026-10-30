use super::collection::COLLECTION;
use super::store_err::store_err;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::labels::model::LabelTemplateDocument;

use mongodb::bson::doc;

/// Deletes a template; `false` when the key does not exist.
pub(crate) async fn delete_template(db: &Db, key: &str) -> AppResult<bool> {
    match db {
        Db::Sqlite(pool) => {
            let result = sqlx::query("DELETE FROM label_templates WHERE key = ?1")
                .bind(key)
                .execute(pool)
                .await
                .map_err(|e| store_err("delete", e))?;
            Ok(result.rows_affected() > 0)
        }
        Db::Mongo(mongo) => Ok(mongo
            .collection::<LabelTemplateDocument>(COLLECTION)
            .delete_one(doc! { "key": key })
            .await?
            .deleted_count
            > 0),
    }
}
