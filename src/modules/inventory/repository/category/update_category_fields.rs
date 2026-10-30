use super::categories::categories;
use super::find_category_by_key::find_category_by_key;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::inventory::model::CategoryDocument;

use mongodb::bson::{Document, doc};
use mongodb::options::ReturnDocument;

/// `set_doc` is assembled by the caller (`service::category::update_category`);
/// this just applies it, filtered by the category's immutable `key` rather
/// than its (renameable) `name`.
pub(crate) async fn update_category_fields(
    db: &Db,
    key: &str,
    set_doc: Document,
) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db)
            .find_one_and_update(doc! { "key": key }, doc! { "$set": set_doc })
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let existing = find_category_by_key(db, key).await?;
            let Some(mut cat) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                cat.name = name.to_string();
            }
            if let Ok(icon) = set_doc.get_str("icon") {
                cat.icon = icon.to_string();
            }
            if let Ok(color) = set_doc.get_str("color") {
                cat.color = color.to_string();
            }
            if let Ok(dt) = set_doc.get_datetime("updated_at") {
                cat.updated_at = *dt;
            }
            cat.version += 1;

            let updated_at_iso = bson_to_iso(&cat.updated_at);

            sqlx::query(
                "UPDATE categories SET name = ?, icon = ?, color = ?, version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(&cat.name)
            .bind(&cat.icon)
            .bind(&cat.color)
            .bind(cat.version)
            .bind(&updated_at_iso)
            .bind(key)
            .execute(pool)
            .await?;

            Ok(Some(cat))
        }
    }
}
