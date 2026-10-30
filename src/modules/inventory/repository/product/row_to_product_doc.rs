use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn row_to_product_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProductDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let barcode_source_str: Option<String> = row.get("barcode_source");
    let barcode_source = barcode_source_str.and_then(|s| s.parse().ok());
    let is_serialized_int: i64 = row.get("is_serialized");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(ProductDocument {
        id: object_id,
        key: row.get("key"),
        sku: row.get("sku"),
        barcode: row.get("barcode"),
        barcode_source,
        name: row.get("name"),
        category_key: row.get("category_key"),
        subcategory_key: row.get("subcategory_key"),
        stock_quantity: row.get("stock_quantity"),
        min_stock_threshold: row.get("min_stock_threshold"),
        is_serialized: is_serialized_int != 0,
        warranty_months: row.get("warranty_months"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
