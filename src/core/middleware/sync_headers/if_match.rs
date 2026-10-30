use crate::core::error::AppError;
use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;

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
