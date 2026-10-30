use super::app_error::AppError;

/// Handler and service-layer return type: every fallible operation in this
/// codebase resolves to either a domain value or an `AppError` that already
/// knows how to render itself as the right HTTP response.
pub type AppResult<T> = Result<T, AppError>;
