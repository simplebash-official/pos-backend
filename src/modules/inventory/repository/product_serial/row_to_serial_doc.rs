use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::domain::inventory::SerialStatus;
use crate::modules::inventory::model::ProductSerialDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn row_to_serial_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProductSerialDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let status_str: String = row.get("status");
    let status = status_str.parse().unwrap_or(SerialStatus::InStock);
    let sold_at_str: Option<String> = row.get("sold_at");
    let warranty_expires_at_str: Option<String> = row.get("warranty_expires_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(ProductSerialDocument {
        id: object_id,
        key: row.get("key"),
        product_key: row.get("product_key"),
        serial_number: row.get("serial_number"),
        batch_key: row.get("batch_key"),
        status,
        invoice_key: row.get("invoice_key"),
        sold_at: sold_at_str.map(|s| to_bson_datetime(&s)),
        warranty_months: row.get("warranty_months"),
        warranty_expires_at: warranty_expires_at_str.map(|s| to_bson_datetime(&s)),
        credit_note_key: row.get("credit_note_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}
