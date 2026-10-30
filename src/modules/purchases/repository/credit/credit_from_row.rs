use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::{AppError, AppResult};
use crate::domain::supplier_credits::CreditReason;
use crate::modules::purchases::model::SupplierCreditDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn credit_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<SupplierCreditDocument> {
    let reason: String = row.get("reason");
    Ok(SupplierCreditDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        supplier_key: row.get("supplier_key"),
        receipt_key: row.get("receipt_key"),
        amount_cents: row.get("amount_cents"),
        reason: CreditReason::parse(&reason).ok_or_else(|| {
            AppError::internal(format!("Unknown credit reason '{reason}' in database"))
        })?,
        note: row.get("note"),
        reverses_key: row.get("reverses_key"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}
