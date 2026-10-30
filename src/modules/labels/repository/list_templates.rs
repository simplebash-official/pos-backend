use super::collection::COLLECTION;
use super::row_to_doc::row_to_doc;
use super::store_err::store_err;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::labels::model::LabelTemplateDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every template the shop built, ordered by name.
pub(crate) async fn list_templates(db: &Db) -> AppResult<Vec<LabelTemplateDocument>> {
    match db {
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT key, name, width_mm, height_mm, elements, version, created_at, updated_at \
                 FROM label_templates ORDER BY name COLLATE NOCASE",
            )
            .fetch_all(pool)
            .await
            .map_err(|e| store_err("list", e))?;
            rows.iter().map(row_to_doc).collect()
        }
        Db::Mongo(mongo) => {
            let mut templates: Vec<LabelTemplateDocument> = mongo
                .collection::<LabelTemplateDocument>(COLLECTION)
                .find(doc! {})
                .await?
                .try_collect()
                .await?;
            templates.sort_by_key(|t| t.name.to_lowercase());
            Ok(templates)
        }
    }
}
