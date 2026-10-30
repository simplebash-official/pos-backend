use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::balances::BalanceSubject;
use crate::modules::balances::model::BalanceOpeningDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn opening_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<BalanceOpeningDocument> {
    let kind: String = row.get("subject_kind");
    Ok(BalanceOpeningDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        subject_kind: BalanceSubject::parse(&kind).ok_or_else(|| {
            AppError::internal(format!("Unknown balance subject '{kind}' in database"))
        })?,
        subject_key: row.get("subject_key"),
        opening_balance_cents: row.get("opening_balance_cents"),
        as_of_date: row.get("as_of_date"),
        set_by_name_snapshot: row.get("set_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}
