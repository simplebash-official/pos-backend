// Outermost middleware: gives every request a `request_id` (taken from the
// caller's `X-Request-Id` so a frontend click, this request and the
// document-server render it triggers share one id), opens a `request` span
// that every log line inside the handler inherits (`request_id`, `user_id`),
// and logs `http/request` + `http/response` with status, latency, sizes and
// — when `LOG_HTTP_BODIES` is on — the redacted, capped bodies.

use std::time::Instant;

use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, header},
    middleware::Next,
    response::Response,
};
use tracing::{Instrument, Level, field};

use super::{redact, settings};
use crate::{app::AppState, core::middleware::auth::Claims};

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Bodies above this are never buffered for logging (backup imports and
/// PDFs can be tens of MB); only their size is recorded.
const MAX_BUFFERED_BODY_BYTES: u64 = 2 * 1024 * 1024;

tokio::task_local! {
    /// The current request's id, readable from anywhere in the handler's
    /// task — used to forward `X-Request-Id` on outbound calls.
    static REQUEST_ID: String;
}

/// Id of the request being handled on this task, if any.
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok()
}

/// Accept a caller-supplied id only if it is short and plain, so a header
/// can't inject arbitrary text into every log line.
fn incoming_request_id(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(REQUEST_ID_HEADER)?.to_str().ok()?.trim();
    let valid = !raw.is_empty()
        && raw.len() <= 128
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    valid.then(|| raw.to_string())
}

/// Best-effort caller id for the log only. Unlike the auth extractors this
/// never rejects: an invalid token just leaves `user_id` empty.
fn peek_user_id(headers: &HeaderMap, jwt_secret: &str) -> Option<String> {
    let token = headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .ok()
    .map(|data| data.claims.sub)
}

fn header_str(headers: &HeaderMap, name: impl header::AsHeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn is_textual(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|ct| {
        let ct = ct.to_ascii_lowercase();
        ct.starts_with("application/json")
            || ct.starts_with("text/")
            || ct.starts_with("application/x-www-form-urlencoded")
    })
}

/// Should this body be buffered and logged? Only textual bodies of a known,
/// modest length — from `Content-Length`, or the body's exact size hint
/// (the timing middleware rebuilds JSON responses without the header) — so
/// streams and uploads are never held in memory.
fn should_capture(headers: &HeaderMap, body: &Body) -> bool {
    use axum::body::HttpBody;
    let content_type = header_str(headers, header::CONTENT_TYPE);
    let length = header_str(headers, header::CONTENT_LENGTH)
        .and_then(|l| l.parse::<u64>().ok())
        .or_else(|| body.size_hint().exact());
    settings().http_bodies
        && is_textual(content_type.as_deref())
        && length.is_some_and(|l| l <= MAX_BUFFERED_BODY_BYTES)
}

pub async fn log_requests(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let request_id = incoming_request_id(request.headers())
        .unwrap_or_else(|| crate::core::id::generate_id("req"));

    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(str::to_string);
    let user_id = peek_user_id(request.headers(), &state.config.jwt_secret);
    let device_id = header_str(request.headers(), "x-device-id");

    let span = tracing::span!(
        Level::INFO,
        "request",
        request_id = %request_id,
        method = %method,
        path = %path,
        user_id = field::Empty,
        device_id = field::Empty,
    );
    if let Some(user) = &user_id {
        span.record("user_id", user.as_str());
    }
    if let Some(device) = &device_id {
        span.record("device_id", device.as_str());
    }

    let request_id_for_task = request_id.clone();
    let handle = async move {
        let (mut parts, body) = request.into_parts();
        if let Ok(value) = HeaderValue::from_str(&request_id) {
            parts.headers.insert(REQUEST_ID_HEADER, value);
        }

        let req_content_type = header_str(&parts.headers, header::CONTENT_TYPE);
        let req_length = header_str(&parts.headers, header::CONTENT_LENGTH);
        let (body, req_body_log) = if should_capture(&parts.headers, &body) {
            match axum::body::to_bytes(body, MAX_BUFFERED_BODY_BYTES as usize).await {
                Ok(bytes) => {
                    let logged = redact::body_for_log(&bytes, settings().body_cap_bytes);
                    (Body::from(bytes), Some(logged))
                }
                // Length header lied / stream error: nothing left to forward.
                Err(_) => (Body::empty(), Some("[unreadable body]".to_string())),
            }
        } else {
            (body, None)
        };

        tracing::info!(
            target: "http",
            category = "http",
            event = "request",
            method = %method,
            path = %path,
            query = query.as_deref(),
            content_type = req_content_type.as_deref(),
            content_length = req_length.as_deref(),
            user_agent = header_str(&parts.headers, header::USER_AGENT).as_deref(),
            body = req_body_log.as_deref(),
            "{method} {path}"
        );

        let response = next.run(Request::from_parts(parts, body)).await;

        let (mut parts, body) = response.into_parts();
        let res_content_type = header_str(&parts.headers, header::CONTENT_TYPE);
        let (body, res_body_log, res_bytes) = if should_capture(&parts.headers, &body) {
            match axum::body::to_bytes(body, MAX_BUFFERED_BODY_BYTES as usize).await {
                Ok(bytes) => {
                    let logged = redact::body_for_log(&bytes, settings().body_cap_bytes);
                    let len = bytes.len();
                    (Body::from(bytes), Some(logged), Some(len.to_string()))
                }
                Err(_) => (Body::from(Bytes::new()), None, None),
            }
        } else {
            let len = header_str(&parts.headers, header::CONTENT_LENGTH);
            (body, None, len)
        };
        if let Ok(value) = HeaderValue::from_str(&request_id) {
            parts.headers.insert(REQUEST_ID_HEADER, value);
        }

        let status = parts.status.as_u16();
        let latency_ms = (started.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
        macro_rules! response_event {
            ($lvl:expr) => {
                tracing::event!(
                    target: "http",
                    $lvl,
                    category = "http",
                    event = "response",
                    method = %method,
                    path = %path,
                    status,
                    latency_ms,
                    content_type = res_content_type.as_deref(),
                    bytes = res_bytes.as_deref(),
                    body = res_body_log.as_deref(),
                    "{method} {path} → {status} in {latency_ms}ms"
                )
            };
        }
        match status {
            500.. => response_event!(Level::ERROR),
            400..=499 => response_event!(Level::WARN),
            _ => response_event!(Level::INFO),
        }

        Response::from_parts(parts, body)
    };

    REQUEST_ID
        .scope(request_id_for_task, handle.instrument(span))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_header_is_validated() {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, HeaderValue::from_static("req_abc-123"));
        assert_eq!(
            incoming_request_id(&headers).as_deref(),
            Some("req_abc-123")
        );

        headers.insert(
            REQUEST_ID_HEADER,
            HeaderValue::from_static("bad id with spaces"),
        );
        assert_eq!(incoming_request_id(&headers), None);

        let long = "a".repeat(200);
        headers.insert(REQUEST_ID_HEADER, HeaderValue::from_str(&long).unwrap());
        assert_eq!(incoming_request_id(&headers), None);
    }

    #[test]
    fn only_textual_bodies_are_candidates() {
        assert!(is_textual(Some("application/json; charset=utf-8")));
        assert!(is_textual(Some("text/plain")));
        assert!(!is_textual(Some("application/pdf")));
        assert!(!is_textual(Some("multipart/form-data; boundary=x")));
        assert!(!is_textual(None));
    }
}
