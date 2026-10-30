use super::collection::COLLECTION;
use super::store_err::store_err;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;

use crate::modules::labels::model::LabelTemplateDocument;

/// Inserts a new template.
pub(crate) async fn insert_template(db: &Db, template: &LabelTemplateDocument) -> AppResult<()> {
    match db {
        Db::Sqlite(pool) => {
            let elements =
                serde_json::to_string(&template.elements).map_err(|e| store_err("save", e))?;
            sqlx::query(
                "INSERT INTO label_templates \
                 (key, name, width_mm, height_mm, elements, version, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(&template.key)
            .bind(&template.name)
            .bind(template.width_mm)
            .bind(template.height_mm)
            .bind(&elements)
            .bind(template.version)
            .bind(bson_to_iso(&template.created_at))
            .bind(bson_to_iso(&template.updated_at))
            .execute(pool)
            .await
            .map_err(|e| store_err("save", e))?;
            Ok(())
        }
        Db::Mongo(mongo) => {
            mongo
                .collection::<LabelTemplateDocument>(COLLECTION)
                .insert_one(template)
                .await?;
            Ok(())
        }
    }
}
