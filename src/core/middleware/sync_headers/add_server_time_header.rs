use axum::extract::Request;
use axum::http::header;

use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;

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
