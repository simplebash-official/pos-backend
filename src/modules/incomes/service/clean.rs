use crate::core::error::{AppError, AppResult};

pub(super) fn clean(label: &str, value: Option<&str>, max: usize) -> AppResult<Option<String>> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > max {
        return Err(AppError::validation(format!(
            "{label} can have up to {max} letters."
        )));
    }
    Ok(Some(text.to_string()))
}
