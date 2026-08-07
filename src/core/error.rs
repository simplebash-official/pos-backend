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
#[derive(Debug, thiserror::Error)]
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
        }
    }

    /// The single place a variant maps to its wire representation. Centralizing
    /// this (rather than matching in `IntoResponse` directly) keeps the
    /// default-code-per-variant logic in one spot instead of duplicated
    /// wherever an `AppError` needs to be inspected outside of a response.
    pub fn status_code_code_and_message(&self) -> (StatusCode, String, String) {
        match self {
            AppError::NotFound { message, code } => (
                StatusCode::NOT_FOUND,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::NOT_FOUND.to_string()),
                message.clone(),
            ),
            AppError::Validation { message, code } => (
                StatusCode::BAD_REQUEST,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::VALIDATION_ERROR.to_string()),
                message.clone(),
            ),
            AppError::Unauthorized { message, code } => (
                StatusCode::UNAUTHORIZED,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::UNAUTHORIZED.to_string()),
                message.clone(),
            ),
            AppError::Forbidden { message, code } => (
                StatusCode::FORBIDDEN,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::FORBIDDEN.to_string()),
                message.clone(),
            ),
            AppError::Internal { message, code } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                code.clone().unwrap_or_else(|| {
                    crate::core::constants::codes::INTERNAL_SERVER_ERROR.to_string()
                }),
                message.clone(),
            ),
            AppError::Custom {
                status,
                code,
                message,
            } => (*status, code.clone(), message.clone()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.status_code_code_and_message();

        // 5xx failures are unexpected (Mongo down, a bug) — log them
        // server-side since the client only sees the generic message, not
        // the internal detail that ended up in `message` here.
        if status.is_server_error() {
            tracing::error!(code = %code, error = %message, "internal server error");
        }

        let body = ErrorResponse {
            success: false,
            message,
            code,
            status_code: status.as_u16(),
        };

        (status, Json(body)).into_response()
    }
}

// Lets handlers `?`-propagate a Mongo driver error directly into an
// `AppResult` instead of matching on it at every call site. Collapsed to
// `Internal` because a raw driver error (connection drop, query error) is
// never something the caller can act on — it's always a 500.
impl From<mongodb::error::Error> for AppError {
    fn from(err: mongodb::error::Error) -> Self {
        AppError::internal(err.to_string())
    }
}

// Same idea for JWT decode failures (expired/malformed/wrong-signature
// token) — always means "the caller isn't authenticated", so this maps to
// `Unauthorized` rather than distinguishing the underlying JWT error kind.
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::unauthorized(err.to_string())
    }
}

// Same idea for BSON (de)serialization failures against a document we built
// or read ourselves (e.g. deserializing an aggregation pipeline result) —
// never something a caller can act on, always a 500.
impl From<bson::error::Error> for AppError {
    fn from(err: bson::error::Error) -> Self {
        AppError::internal(err.to_string())
    }
}

/// Handler and service-layer return type: every fallible operation in this
/// codebase resolves to either a domain value or an `AppError` that already
/// knows how to render itself as the right HTTP response.
pub type AppResult<T> = Result<T, AppError>;
