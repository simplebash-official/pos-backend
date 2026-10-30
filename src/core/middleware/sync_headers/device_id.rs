use axum::extract::FromRequestParts;

use axum::http::request::Parts;

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
