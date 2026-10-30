use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::expenses::PaidFrom;
use crate::modules::expenses::model::ExpensePaymentDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn payment_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<ExpensePaymentDocument> {
    let from: String = row.get("paid_from");
    Ok(ExpensePaymentDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        expense_key: row.get("expense_key"),
        amount_cents: row.get("amount_cents"),
        paid_from: PaidFrom::parse(&from).ok_or_else(|| {
            AppError::internal(format!("Unknown payment account '{from}' in database"))
        })?,
        paid_on: row.get("paid_on"),
        reference_no: row.get("reference_no"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}
