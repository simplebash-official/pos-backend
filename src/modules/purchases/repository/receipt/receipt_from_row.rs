use crate::clients::sqlite::to_bson_datetime;

use crate::modules::purchases::model::PurchaseReceiptDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn receipt_from_row(row: &sqlx::sqlite::SqliteRow) -> PurchaseReceiptDocument {
    PurchaseReceiptDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        receipt_number: row.get("receipt_number"),
        supplier_key: row.get("supplier_key"),
        reference_no: row.get("reference_no"),
        date: to_bson_datetime(&row.get::<String, _>("date")),
        notes: row.get("notes"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    }
}
