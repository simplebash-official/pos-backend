use axum::http::StatusCode;

/// Standard HTTP Status Code constants re-exported for consistent use across the backend.
pub const OK: StatusCode = StatusCode::OK; // 200
pub const CREATED: StatusCode = StatusCode::CREATED; // 201
pub const BAD_REQUEST: StatusCode = StatusCode::BAD_REQUEST; // 400
pub const UNAUTHORIZED: StatusCode = StatusCode::UNAUTHORIZED; // 401
pub const FORBIDDEN: StatusCode = StatusCode::FORBIDDEN; // 403
pub const NOT_FOUND: StatusCode = StatusCode::NOT_FOUND; // 404
pub const CONFLICT: StatusCode = StatusCode::CONFLICT; // 409
pub const INTERNAL_SERVER_ERROR: StatusCode = StatusCode::INTERNAL_SERVER_ERROR; // 500
