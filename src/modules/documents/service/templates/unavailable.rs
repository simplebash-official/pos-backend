use crate::core::error::AppError;

pub(super) fn unavailable() -> AppError {
    AppError::unprocessable_entity(
        "TEMPLATE_UNAVAILABLE",
        "That design is not available for this document. Choose another design in Settings.",
    )
}
