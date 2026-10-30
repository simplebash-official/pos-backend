use super::resolved_template::ResolvedTemplate;

use crate::core::error::{AppError, AppResult};

/// Adds the payload's `options` object for a design that declares options
/// (its schema requires it); a design without options gets none.
pub(crate) fn with_options(
    mut data: serde_json::Value,
    resolved: &ResolvedTemplate,
) -> AppResult<serde_json::Value> {
    if resolved.info.options.is_empty() {
        return Ok(data);
    }
    match data.as_object_mut() {
        Some(fields) => {
            fields.insert(
                "options".to_string(),
                serde_json::Value::Object(resolved.option_values.clone()),
            );
            Ok(data)
        }
        None => Err(AppError::internal("print payload is not a JSON object")),
    }
}
