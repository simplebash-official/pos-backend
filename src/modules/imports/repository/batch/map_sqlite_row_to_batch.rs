use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::AppResult;
use crate::modules::imports::model::{ImportBatchDocument, ImportRowErrorDocument};
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn map_sqlite_row_to_batch(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<ImportBatchDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let errors_str: Option<String> = row.get("errors");
    let errors: Vec<ImportRowErrorDocument> = match errors_str {
        Some(s) if !s.is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    };
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    let total_rows_i: i64 = row.get("total_rows");
    let successful_rows_i: i64 = row.get("successful_rows");
    let failed_rows_i: i64 = row.get("failed_rows");

    Ok(ImportBatchDocument {
        id,
        key: row.get("key"),
        target: row.get("target"),
        file_name: row.get("file_name"),
        file_type: row.get("file_type"),
        file_size_bytes: row.get("file_size_bytes"),
        total_rows: total_rows_i as u64,
        successful_rows: successful_rows_i as u64,
        failed_rows: failed_rows_i as u64,
        status: row.get("status"),
        errors,
        created_by_user_key: row.get("created_by_user_key"),
        created_by_user_name: row.get("created_by_user_name"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}
