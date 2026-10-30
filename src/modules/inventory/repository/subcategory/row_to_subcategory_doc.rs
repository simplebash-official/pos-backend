use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::inventory::model::SubcategoryDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn row_to_subcategory_doc(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<SubcategoryDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(SubcategoryDocument {
        id: object_id,
        key: row.get("key"),
        category_key: row.get("category_key"),
        name: row.get("name"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
