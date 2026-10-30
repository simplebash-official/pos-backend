use super::request_limits::RequestLimits;

/// Axum middleware: answers 429 once a client exhausts its bucket. The client
/// is nginx's `X-Real-IP` (it overwrites the header, so it can't be spoofed
/// through the proxy); a backend reached directly (desktop sidecar on
/// loopback) has none and shares one "local" bucket.
pub async fn limit_requests(
    axum::extract::State(limits): axum::extract::State<std::sync::Arc<RequestLimits>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let client = req
        .headers()
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local");
    if limits.allow(client, req.uri().path()) {
        next.run(req).await
    } else {
        crate::core::error::AppError::custom(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            crate::core::constants::codes::RATE_LIMITED,
            "Too many requests. Please slow down and try again shortly.",
        )
        .into_response()
    }
}
