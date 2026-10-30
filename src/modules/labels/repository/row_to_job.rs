use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::labels::{LabelJobOutcome, LabelPrintAction, LabelPrintMethod};
use crate::modules::labels::model::{LabelJobItemDocument, LabelPrintJobDocument};

use sqlx::Row;

fn text_enum<T: serde::de::DeserializeOwned>(text: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|e| AppError::internal(format!("Stored label print job is corrupt: {e}")))
}

pub(super) fn row_to_job(row: &sqlx::sqlite::SqliteRow) -> AppResult<LabelPrintJobDocument> {
    let items_json: String = row.get("items");
    let items: Vec<LabelJobItemDocument> = serde_json::from_str(&items_json)
        .map_err(|e| AppError::internal(format!("Stored label print job is corrupt: {e}")))?;
    let method: Option<String> = row.get("method");
    let dpi: Option<i64> = row.get("dpi");
    let total_labels: i64 = row.get("total_labels");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(LabelPrintJobDocument {
        id: None,
        key: row.get("key"),
        template_key: row.get("template_key"),
        template_name: row.get("template_name"),
        width_mm: row.get("width_mm"),
        height_mm: row.get("height_mm"),
        action: text_enum::<LabelPrintAction>(&row.get::<String, _>("action"))?,
        method: method
            .as_deref()
            .map(text_enum::<LabelPrintMethod>)
            .transpose()?,
        printer_name: row.get("printer_name"),
        dpi: dpi.map(|dpi| dpi as u32),
        outcome: text_enum::<LabelJobOutcome>(&row.get::<String, _>("outcome"))?,
        error: row.get("error"),
        printed_by_user_id: row.get("printed_by_user_id"),
        printed_by_name: row.get("printed_by_name"),
        items,
        total_labels: total_labels as u32,
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
    })
}
