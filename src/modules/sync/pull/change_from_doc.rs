use crate::core::error::{AppError, AppResult};

use crate::domain::sync_v2::{ChangeOp, ChangeRecord, PulledChange};

use mongodb::bson::Document;
use serde_json::Value;

/// Reads one stored `sync_changes` row back into its wire form.
pub(crate) fn change_from_doc(row: &Document) -> AppResult<PulledChange> {
    let text = |field: &str| -> AppResult<String> {
        row.get_str(field)
            .map(str::to_string)
            .map_err(|_| AppError::internal(format!("stored change has no '{field}'")))
    };
    let op = match row.get_str("op") {
        Ok("delete") => ChangeOp::Delete,
        _ => ChangeOp::Upsert,
    };
    let updated_at = row
        .get_datetime("updated_at")
        .map(|d| d.to_chrono())
        .map_err(|_| AppError::internal("stored change has no 'updated_at'"))?;
    let payload = match row.get("payload") {
        Some(mongodb::bson::Bson::Document(p)) => Some(
            serde_json::to_value(p)
                .map_err(|e| AppError::internal(format!("stored payload is unreadable: {e}")))?,
        ),
        _ => None,
    };
    Ok(PulledChange {
        seq: row.get_i64("seq").unwrap_or_default(),
        origin_device_id: row.get_str("origin_device_id").ok().map(str::to_string),
        record: ChangeRecord {
            resource: text("resource")?,
            key: text("key")?,
            op,
            version: crate::modules::sync::apply_mongo::num_i64(row, "version").unwrap_or(1),
            updated_at,
            device_id: row.get_str("device_id").unwrap_or("cloud").to_string(),
            payload: payload.filter(|v: &Value| v.is_object()),
        },
    })
}
