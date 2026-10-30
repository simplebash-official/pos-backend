use crate::core::error::{AppError, AppResult};

const NOTE_MAX: usize = 200;

pub(super) fn clean_note(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > NOTE_MAX {
        return Err(AppError::validation(format!(
            "The note can have up to {NOTE_MAX} letters."
        )));
    }
    Ok(Some(text.to_string()))
}
