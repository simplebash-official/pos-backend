use super::name_max::NAME_MAX;
use crate::core::error::{AppError, AppResult};

const NAME_MIN: usize = 2;

pub(super) fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = name.chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&len) {
        return Err(AppError::validation(format!(
            "A category name needs {NAME_MIN} to {NAME_MAX} letters."
        )));
    }
    Ok(name)
}
