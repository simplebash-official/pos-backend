use axum::http::StatusCode;

/// How an `AppError` appears on the wire: the HTTP status and the error body fields.
#[derive(Debug)]
pub struct WireParts {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}
