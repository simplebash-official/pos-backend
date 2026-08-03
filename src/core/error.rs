use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::core::response::ErrorResponse;

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

    pub fn custom(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Custom {
            status,
            code: code.into(),
            message: message.into(),
        }
    }

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

impl From<mongodb::error::Error> for AppError {
    fn from(err: mongodb::error::Error) -> Self {
        AppError::internal(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::unauthorized(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
