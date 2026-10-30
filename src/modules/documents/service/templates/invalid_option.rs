use crate::core::error::AppError;

pub(super) fn invalid_option(message: String) -> AppError {
    AppError::unprocessable_entity("TEMPLATE_OPTIONS_INVALID", message)
}
