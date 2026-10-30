use crate::clients::sqlite::to_bson_datetime;
use crate::modules::expenses::model::ExpenseDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn expense_from_row(row: &sqlx::sqlite::SqliteRow) -> ExpenseDocument {
    ExpenseDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        expense_number: row.get("expense_number"),
        expense_date: row.get("expense_date"),
        category_key: row.get("category_key"),
        payee_key: row.get("payee_key"),
        payee_name: row.get("payee_name"),
        amount_cents: row.get("amount_cents"),
        due_date: row.get("due_date"),
        reference_no: row.get("reference_no"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    }
}
