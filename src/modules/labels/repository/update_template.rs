use super::collection::COLLECTION;
use super::find_template_by_key::find_template_by_key;
use super::store_err::store_err;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::domain::labels::LabelElement;
use crate::modules::labels::model::LabelTemplateDocument;

use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Replaces a template's design and bumps its version; `None` when the key
/// does not exist.
pub(crate) async fn update_template(
    db: &Db,
    key: &str,
    name: &str,
    width_mm: f64,
    height_mm: f64,
    elements: &[LabelElement],
) -> AppResult<Option<LabelTemplateDocument>> {
    let now = BsonDateTime::now();
    match db {
        Db::Sqlite(pool) => {
            let elements_json =
                serde_json::to_string(elements).map_err(|e| store_err("save", e))?;
            let result = sqlx::query(
                "UPDATE label_templates SET name = ?2, width_mm = ?3, height_mm = ?4, \
                 elements = ?5, version = version + 1, updated_at = ?6 WHERE key = ?1",
            )
            .bind(key)
            .bind(name)
            .bind(width_mm)
            .bind(height_mm)
            .bind(&elements_json)
            .bind(bson_to_iso(&now))
            .execute(pool)
            .await
            .map_err(|e| store_err("save", e))?;
            if result.rows_affected() == 0 {
                return Ok(None);
            }
            find_template_by_key(db, key).await
        }
        Db::Mongo(mongo) => {
            let elements_bson =
                mongodb::bson::serialize_to_bson(elements).map_err(|e| store_err("save", e))?;
            Ok(mongo
                .collection::<LabelTemplateDocument>(COLLECTION)
                .find_one_and_update(
                    doc! { "key": key },
                    doc! {
                        "$set": {
                            "name": name,
                            "width_mm": width_mm,
                            "height_mm": height_mm,
                            "elements": elements_bson,
                            "updated_at": now,
                        },
                        "$inc": { "version": 1 },
                    },
                )
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
    }
}
