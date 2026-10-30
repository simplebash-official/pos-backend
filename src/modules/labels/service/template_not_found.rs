use crate::core::error::AppError;

const TEMPLATE_NOT_FOUND: &str = "LABEL_TEMPLATE_NOT_FOUND";

pub(super) fn template_not_found() -> AppError {
    AppError::not_found_with_code("Label design not found", TEMPLATE_NOT_FOUND)
}
