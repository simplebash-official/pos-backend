use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::inventory::model::CategoryDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn row_to_category_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<CategoryDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(CategoryDocument {
        id: object_id,
        key: row.get("key"),
        name: row.get("name"),
        icon: row.get("icon"),
        color: row.get("color"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
