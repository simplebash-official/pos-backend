use axum::http::StatusCode;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Unified API error response payload. Also gets `processingTimeMs` spliced
/// in by `core::middleware::timing` — see `ApiResponse` above.
///
/// Example JSON:
/// ```json
/// {
///   "success": false,
///   "message": "Product not found",
///   "code": "PRODUCT_NOT_FOUND",
///   "statusCode": 404,
///   "processingTimeMs": 3
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
    pub code: String,
    pub status_code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            code: code.into(),
            status_code: status.as_u16(),
            details: None,
        }
    }

    pub fn with_details(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            success: false,
            message: message.into(),
            code: code.into(),
            status_code: status.as_u16(),
            details: Some(details),
        }
    }
}
