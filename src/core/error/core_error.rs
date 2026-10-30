use super::app_error::AppError;
use axum::http::StatusCode;

/// Handler and service-layer return type: every fallible operation in this
/// codebase resolves to either a domain value or an `AppError` that already
/// knows how to render itself as the right HTTP response.
pub type AppResult<T> = Result<T, AppError>;

/// How an `AppError` appears on the wire: the HTTP status and the error body fields.
#[derive(Debug)]
pub struct WireParts {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}
