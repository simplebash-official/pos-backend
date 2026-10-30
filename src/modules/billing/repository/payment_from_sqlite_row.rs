use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;

use crate::modules::billing::model::PaymentDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(crate) fn payment_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<PaymentDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let recorded_at_str: String = row.get("recorded_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(PaymentDocument {
        id,
        key: row.get("key"),
        invoice_key: row.get("invoice_key"),
        amount_cents: row.get("amount_cents"),
        payment_method: row.get("payment_method"),
        notes: row.get("notes"),
        session_key: row.get("session_key"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        recorded_at: to_bson_datetime(&recorded_at_str),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}
