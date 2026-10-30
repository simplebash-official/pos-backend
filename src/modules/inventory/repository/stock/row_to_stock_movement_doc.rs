use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::domain::inventory::StockMovementType;
use crate::modules::inventory::model::StockMovementDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn row_to_stock_movement_doc(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<StockMovementDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let product_id_str: String = row.get("product_id");
    let product_id = ObjectId::parse_str(&product_id_str).unwrap_or_default();
    let movement_type_str: String = row.get("movement_type");
    let movement_type = movement_type_str
        .parse()
        .unwrap_or(StockMovementType::ManualAdjustment);
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(StockMovementDocument {
        id: object_id,
        key: row.get("key"),
        product_id,
        batch_key: row.get("batch_key"),
        quantity_delta: row.get("quantity_delta"),
        movement_type,
        reference_id: row.get("reference_id"),
        note: row.get("note"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
