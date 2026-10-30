use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn row_to_batch_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<StockBatchDocument> {
    let id_str: String = row.get("id");
    let source_str: String = row.get("source");
    let received_at_str: String = row.get("received_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(StockBatchDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        product_key: row.get("product_key"),
        source: source_str.parse()?,
        purchase_key: row.get("purchase_key"),
        supplier_key: row.get("supplier_key"),
        received_quantity: row.get("received_quantity"),
        remaining_quantity: row.get("remaining_quantity"),
        unit_cost_cents: row.get("unit_cost_cents"),
        selling_price_cents: row.get("selling_price_cents"),
        received_at: to_bson_datetime(&received_at_str),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
