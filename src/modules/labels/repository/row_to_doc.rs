use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::labels::LabelElement;
use crate::modules::labels::model::LabelTemplateDocument;

use sqlx::Row;

pub(super) fn row_to_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<LabelTemplateDocument> {
    let elements_json: String = row.get("elements");
    let elements: Vec<LabelElement> = serde_json::from_str(&elements_json)
        .map_err(|e| AppError::internal(format!("Stored label template is corrupt: {e}")))?;
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(LabelTemplateDocument {
        id: None,
        key: row.get("key"),
        name: row.get("name"),
        width_mm: row.get("width_mm"),
        height_mm: row.get("height_mm"),
        elements,
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
    })
}
