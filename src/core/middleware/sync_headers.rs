use axum::{
    extract::{FromRequestParts, Request},
    http::{header, request::Parts},
    middleware::Next,
    response::Response,
};
use chrono::Utc;

use crate::core::error::AppError;

/// Axum middleware that injects the `X-Server-Time` ISO-8601 UTC timestamp header
/// onto every single HTTP response.
pub async fn add_server_time_header(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if let Ok(val) = header::HeaderValue::from_str(&now) {
        response
            .headers_mut()
            .insert(header::HeaderName::from_static("x-server-time"), val);
    }
    response
}

/// Extractor for the optional `X-Device-Id` header identifying the client terminal.
#[derive(Debug, Clone, Default)]
pub struct DeviceId(pub Option<String>);

impl<S> FromRequestParts<S> for DeviceId
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let device_id = parts
            .headers
            .get("x-device-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(DeviceId(device_id))
    }
}

/// Extractor for the optional `If-Match` header holding the expected entity `version`.
#[derive(Debug, Clone, Copy, Default)]
pub struct IfMatch(pub Option<i64>);

impl<S> FromRequestParts<S> for IfMatch
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(val) = parts.headers.get(header::IF_MATCH) {
            let s = val
                .to_str()
                .map_err(|_| AppError::validation("Invalid If-Match header: non-ASCII string"))?;
            let trimmed = s.trim().trim_matches('"');
            let version = trimmed.parse::<i64>().map_err(|_| {
                AppError::validation(format!(
                    "Invalid If-Match version: '{trimmed}' is not a valid version integer"
                ))
            })?;
            Ok(IfMatch(Some(version)))
        } else {
            Ok(IfMatch(None))
        }
    }
}
