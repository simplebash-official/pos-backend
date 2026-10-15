// Mongo and SQLite access for the `label_templates` collection/table. Like every
// repository it never turns a miss into an error: `service` decides what a
// missing template means.

use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, to_bson_datetime},
    },
    core::error::{AppError, AppResult},
    domain::labels::LabelElement,
    modules::labels::model::LabelTemplateDocument,
};

const COLLECTION: &str = "label_templates";

fn store_err(action: &str, e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Failed to {action} label template: {e}"))
}

fn row_to_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<LabelTemplateDocument> {
    let elements_json: String = row.get("elements");
    let elements: Vec<LabelElement> = serde_json::from_str(&elements_json)
        .map_err(|e| AppError::internal(format!("Stored label template is corrupt: {e}")))?;
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(LabelTemplateDocument {
        id: None,
        key: row.get("key"),
        name: row.get("name"),
        width_mm: row.get("width_mm"),
        height_mm: row.get("height_mm"),
        elements,
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
    })
}

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
