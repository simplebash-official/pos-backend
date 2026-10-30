use super::name_max::NAME_MAX;
use crate::core::error::{AppError, AppResult};

pub(super) fn clean_name_si(raw: &str) -> AppResult<Option<String>> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::validation(format!(
            "The Sinhala name can have up to {NAME_MAX} letters."
        )));
    }
    Ok(Some(name))
}
