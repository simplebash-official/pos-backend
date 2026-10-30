use super::collection::COLLECTION;
use super::row_to_doc::row_to_doc;
use super::store_err::store_err;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::labels::model::LabelTemplateDocument;

use mongodb::bson::doc;

/// One template by its `ltpl_` key.
pub(crate) async fn find_template_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<LabelTemplateDocument>> {
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT key, name, width_mm, height_mm, elements, version, created_at, updated_at \
                 FROM label_templates WHERE key = ?1 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|e| store_err("fetch", e))?;
            row.as_ref().map(row_to_doc).transpose()
        }
        Db::Mongo(mongo) => Ok(mongo
            .collection::<LabelTemplateDocument>(COLLECTION)
            .find_one(doc! { "key": key })
            .await?),
    }
}
