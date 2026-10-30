use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::{AppError, AppResult};

use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn row_to_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<QuotationDocument> {
    let id_str: String = row.get("id");
    let items_json: String = row.get("items");
    let snapshot_json: String = row.get("shop_profile_snapshot");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    let deleted_at: Option<String> = row.get("deleted_at");
    let fulfilled_by_invoice_key: Option<String> =
        row.try_get("fulfilled_by_invoice_key").ok().flatten();
    let fulfilled_at: Option<String> = row.try_get("fulfilled_at").ok().flatten();
    let has_out_of_stock_items: bool = row.try_get("has_out_of_stock_items").unwrap_or(false);

    Ok(QuotationDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        quotation_number: row.get("quotation_number"),
        status: row.get("status"),
        has_out_of_stock_items,
        fulfilled_by_invoice_key,
        fulfilled_at: fulfilled_at.map(|s| to_bson_datetime(&s)),
        customer_key: row.get("customer_key"),
        customer_name: row.get("customer_name"),
        customer_phone: row.get("customer_phone"),
        customer_address: row.get("customer_address"),
        items: serde_json::from_str(&items_json)
            .map_err(|e| AppError::internal(format!("Stored quotation items are corrupt: {e}")))?,
        subtotal_cents: row.get("subtotal_cents"),
        discount_type: row.get("discount_type"),
        discount_value: row.get("discount_value"),
        discount_cents: row.get("discount_cents"),
        total_cents: row.get("total_cents"),
        valid_until: row.get("valid_until"),
        notes: row.get("notes"),
        terms: row.get("terms"),
        prepared_by: row.get("prepared_by"),
        shop_profile_snapshot: serde_json::from_str(&snapshot_json).map_err(|e| {
            AppError::internal(format!("Stored quotation shop profile is corrupt: {e}"))
        })?,
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
        version: row.get("version"),
        deleted_at: deleted_at.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
