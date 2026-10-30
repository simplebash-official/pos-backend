use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::expenses::PaidFrom;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<RecurringExpenseDocument> {
    let from: String = row.get("paid_from");
    Ok(RecurringExpenseDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        category_key: row.get("category_key"),
        payee_key: row.get("payee_key"),
        amount_cents: row.get("amount_cents"),
        due_day: row.get("due_day"),
        remind_days_before: row.get("remind_days_before"),
        paid_from: PaidFrom::parse(&from).ok_or_else(|| {
            AppError::internal(format!("Unknown account '{from}' on a regular bill"))
        })?,
        note: row.get("note"),
        active: row.get::<i64, _>("active") != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}
