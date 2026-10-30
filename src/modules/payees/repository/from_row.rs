use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::PayeeType;
use crate::modules::payees::model::PayeeDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<PayeeDocument> {
    let type_str: String = row.get("payee_type");
    Ok(PayeeDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        payee_type: PayeeType::parse(&type_str).ok_or_else(|| {
            AppError::internal(format!("Unknown payee type '{type_str}' in database"))
        })?,
        name: row.get("name"),
        normalized_name: row.get("normalized_name"),
        phone: row.get("phone"),
        active: row.get::<i64, _>("active") != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}
