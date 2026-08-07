use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Unified API success response wrapper. `core::middleware::timing` splices
/// a `processingTimeMs` field onto every JSON response body (this and
/// `ErrorResponse` alike) after the fact, so it's not a struct field here —
/// it isn't known until the whole request has finished processing.
///
/// Example JSON:
/// ```json
/// {
///   "success": true,
///   "data": { ... },
///   "message": "Product retrieved successfully",
///   "processingTimeMs": 12
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    /// Create a success response with data and a message.
    pub fn success(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
        }
    }

    /// Create a success response with data only.
    pub fn data(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    /// Create a success response with message only.
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: None,
            message: Some(message.into()),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

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
}

impl ErrorResponse {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            code: code.into(),
            status_code: status.as_u16(),
        }
    }
}
