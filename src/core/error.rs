use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::core::response::ErrorResponse;

/// The single error type every handler's `AppResult<T>` returns through.
/// Five named variants (`NotFound`, `Validation`, `Unauthorized`,
/// `Forbidden`, `Internal`) cover the common HTTP statuses and default to a
/// generic error `code` (see `status_code_code_and_message`) when none is
/// given via the `*_with_code` constructors; `Custom` exists as an escape
/// hatch for statuses/codes that don't fit those five (e.g. 409 Conflict,
/// used throughout `inventory` for uniqueness/in-use guards). Implements
/// `IntoResponse` directly, so `?`-propagating one of these from a handler
/// is enough to produce the right HTTP response — no separate mapping step.
#[derive(Clone, Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    NotFound {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Validation {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Unauthorized {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Forbidden {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Internal {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Custom {
        status: StatusCode,
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
}

impl AppError {
    // Each of the five named variants gets a plain constructor (uses the
    // variant name upper-cased as the default `code`, e.g. `NOT_FOUND`) and
    // a `_with_code` constructor (for a module-specific code like
    // `PRODUCT_NOT_FOUND`). Handlers reach for the plain form unless a
    // caller needs to distinguish this particular failure by `code` in the
    // response JSON.
    pub fn not_found(message: impl Into<String>) -> Self {
        AppError::NotFound {
            message: message.into(),
            code: None,
        }
    }

    pub fn not_found_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::NotFound {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        AppError::Validation {
            message: message.into(),
            code: None,
        }
    }

    pub fn validation_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Validation {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        AppError::Unauthorized {
            message: message.into(),
            code: None,
        }
    }

    pub fn unauthorized_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Unauthorized {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        AppError::Forbidden {
            message: message.into(),
            code: None,
        }
    }

    pub fn forbidden_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Forbidden {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal {
            message: message.into(),
            code: None,
        }
    }

    pub fn internal_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Internal {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    /// Escape hatch for any (status, code, message) combination the five
    /// named variants don't cover — e.g. 409 Conflict for uniqueness/
    /// in-use guards, which has no dedicated `AppError` variant.
    pub fn custom(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Custom {
            status,
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn custom_with_details(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        AppError::Custom {
            status,
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::custom(StatusCode::CONFLICT, code, message)
    }

    pub fn conflict_with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        AppError::custom_with_details(StatusCode::CONFLICT, code, message, details)
    }

    pub fn unprocessable_entity(code: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::custom(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    pub fn unprocessable_entity_with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        AppError::custom_with_details(StatusCode::UNPROCESSABLE_ENTITY, code, message, details)
    }

    /// The single place a variant maps to its wire representation. Centralizing
    /// this (rather than matching in `IntoResponse` directly) keeps the
    /// default-code-per-variant logic in one spot instead of duplicated
    /// wherever an `AppError` needs to be inspected outside of a response.
    pub fn status_code_code_message_and_details(
        &self,
    ) -> (StatusCode, String, String, Option<serde_json::Value>) {
        match self {
            AppError::NotFound { message, code } => (
                StatusCode::NOT_FOUND,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::NOT_FOUND.to_string()),
                message.clone(),
                None,
            ),
            AppError::Validation { message, code } => (
                StatusCode::BAD_REQUEST,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::VALIDATION_ERROR.to_string()),
                message.clone(),
                None,
            ),
            AppError::Unauthorized { message, code } => (
                StatusCode::UNAUTHORIZED,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::UNAUTHORIZED.to_string()),
                message.clone(),
                None,
            ),
            AppError::Forbidden { message, code } => (
                StatusCode::FORBIDDEN,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::FORBIDDEN.to_string()),
                message.clone(),
                None,
            ),
            AppError::Internal { message, code } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                code.clone().unwrap_or_else(|| {
                    crate::core::constants::codes::INTERNAL_SERVER_ERROR.to_string()
                }),
                message.clone(),
                None,
            ),
            AppError::Custom {
                status,
                code,
                message,
                details,
            } => (*status, code.clone(), message.clone(), details.clone()),
        }
    }

    pub fn status_code_code_and_message(&self) -> (StatusCode, String, String) {
        let (status, code, message, _) = self.status_code_code_message_and_details();
        (status, code, message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = self.status_code_code_message_and_details();

        let client_message = if status.is_server_error() {
            // 5xx failures are unexpected — log the technical detail server-side
            tracing::error!(code = %code, error = %message, "internal server error");

            // Sanitize message so technical driver strings, SQL errors, or stack details never leak to clients
            sanitize_server_error_message(&message)
        } else {
            message
        };

        let body = ErrorResponse {
            success: false,
            message: client_message,
            code,
            status_code: status.as_u16(),
            details,
        };

        (status, Json(body)).into_response()
    }
}

/// Sanitizes 5xx server messages to prevent internal driver or database details from leaking to clients.
fn sanitize_server_error_message(msg: &str) -> String {
    let lower = msg.to_lowercase();
    if msg.trim().is_empty()
        || lower.contains("error returned from database")
        || lower.contains("constraint failed")
        || lower.contains("syntax error")
        || lower.contains("connection refused")
        || lower.contains("sqlx")
        || lower.contains("sqlite")
        || lower.contains("mongodb")
        || lower.contains("bson")
        || lower.contains("not null")
        || lower.contains("foreign key")
        || lower.contains("unique constraint")
        || lower.contains("table info")
        || lower.contains("driver")
        || lower.contains("no such table")
        || lower.contains("no such column")
        || lower.contains("duplicate key")
        || lower.contains("broken pipe")
        || lower.contains("timeout")
    {
        "An unexpected internal server error occurred. Please try again later or contact support."
            .to_string()
    } else {
        msg.to_string()
    }
}

// Lets handlers `?`-propagate a Mongo driver error directly into an
// `AppResult` instead of matching on it at every call site. Collapsed to
// `Internal` with full technical details logged on the server.
impl From<mongodb::error::Error> for AppError {
    fn from(err: mongodb::error::Error) -> Self {
        tracing::error!(error = %err, "database error (mongodb)");
        AppError::internal("A database error occurred. Please try again later.")
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = %err, "database error (sqlite)");
        AppError::internal("A database error occurred. Please try again later.")
    }
}

// JWT decode failures (expired/malformed/wrong-signature token) — always means
// "the caller isn't authenticated", so this maps to a clean `Unauthorized` message.
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        tracing::warn!(error = %err, "JWT authentication failure");
        AppError::unauthorized("Invalid or expired authentication token. Please log in again.")
    }
}

// BSON (de)serialization failures against a document we built or read ourselves.
impl From<bson::error::Error> for AppError {
    fn from(err: bson::error::Error) -> Self {
        tracing::error!(error = %err, "BSON serialization error");
        AppError::internal("An error occurred while processing data. Please try again.")
    }
}

// JSON serialization of a value we constructed ourselves.
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        tracing::error!(error = %err, "JSON serialization error");
        AppError::internal("An error occurred while processing data. Please try again.")
    }
}

/// Handler and service-layer return type: every fallible operation in this
/// codebase resolves to either a domain value or an `AppError` that already
/// knows how to render itself as the right HTTP response.
pub type AppResult<T> = Result<T, AppError>;
