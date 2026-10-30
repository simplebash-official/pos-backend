use super::job_err::job_err;

use crate::core::error::AppResult;

/// An enum's stored text (its serde name), shared by SQLite columns and filters.
pub(super) fn enum_text<T: serde::Serialize>(value: &T) -> AppResult<String> {
    match serde_json::to_value(value).map_err(|e| job_err("save", e))? {
        serde_json::Value::String(text) => Ok(text),
        other => Err(job_err("save", format!("unexpected value {other}"))),
    }
}
