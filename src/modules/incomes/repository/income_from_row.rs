use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::incomes::ReceivedInto;
use crate::modules::incomes::model::IncomeDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn income_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<IncomeDocument> {
    let into: String = row.get("received_into");
    Ok(IncomeDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        income_number: row.get("income_number"),
        income_date: row.get("income_date"),
        category_key: row.get("category_key"),
        payee_key: row.get("payee_key"),
        payee_name: row.get("payee_name"),
        amount_cents: row.get("amount_cents"),
        received_into: ReceivedInto::parse(&into)
            .ok_or_else(|| AppError::internal(format!("Unknown account '{into}' in database")))?,
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
