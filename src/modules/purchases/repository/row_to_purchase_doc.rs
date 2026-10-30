use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn row_to_purchase_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<PurchaseDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let date_str: String = row.get("date");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(PurchaseDocument {
        id: object_id,
        key: row.get("key"),
        supplier_key: row.get("supplier_key"),
        product_key: row.get("product_key"),
        quantity: row.get("quantity"),
        line_total_cents: row.get("line_total_cents"),
        charges_share_cents: row.get("charges_share_cents"),
        selling_price_cents: row.get("selling_price_cents"),
        date: to_bson_datetime(&date_str),
        receipt_key: row.get("receipt_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
