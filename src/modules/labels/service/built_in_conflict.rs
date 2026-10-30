use crate::core::error::AppError;

use axum::http::StatusCode;

const TEMPLATE_BUILT_IN: &str = "LABEL_TEMPLATE_BUILT_IN";

pub(super) fn built_in_conflict() -> AppError {
    AppError::custom(
        StatusCode::CONFLICT,
        TEMPLATE_BUILT_IN,
        "A ready-made design cannot be changed. Copy it to make your own.",
    )
}
