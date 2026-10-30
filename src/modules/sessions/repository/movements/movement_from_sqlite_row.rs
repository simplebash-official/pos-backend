use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::{CashDirection, CashMovementType};
use crate::modules::sessions::model::CashMovementDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(crate) fn movement_from_sqlite_row(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<CashMovementDocument> {
    let id_str: String = row.get("id");
    let movement_type_str: String = row.get("movement_type");
    let movement_type = CashMovementType::parse(&movement_type_str).ok_or_else(|| {
        AppError::internal(format!(
            "Unknown cash movement type '{movement_type_str}' in database"
        ))
    })?;
    let direction_str: String = row.get("direction");
    let direction = CashDirection::parse(&direction_str).ok_or_else(|| {
        AppError::internal(format!(
            "Unknown cash movement direction '{direction_str}' in database"
        ))
    })?;
    let recorded_late_int: i64 = row.get("recorded_late");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(CashMovementDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        movement_number: row.get("movement_number"),
        movement_type,
        direction,
        amount_cents: row.get("amount_cents"),
        session_key: row.get("session_key"),
        terminal_id: row.get("terminal_id"),
        category_key: row.get("category_key"),
        custom_label: row.get("custom_label"),
        payee_key: row.get("payee_key"),
        payee_name: row.get("payee_name"),
        period_start: row.get("period_start"),
        period_end: row.get("period_end"),
        reference_no: row.get("reference_no"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        approved_by_user_id: row.get("approved_by_user_id"),
        approved_by_name_snapshot: row.get("approved_by_name_snapshot"),
        approval_reason: row.get("approval_reason"),
        reverses_key: row.get("reverses_key"),
        recorded_late: recorded_late_int != 0,
        original_variance_session_key: row.get("original_variance_session_key"),
        receipt_key: row.get("receipt_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}
