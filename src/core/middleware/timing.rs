// Axum middleware that measures how long each request took to process and
// injects the result into the JSON response body as `processingTimeMs`.
// Applied once at the router level (see `app::build_router`) so every route
// — success (`ApiResponse<T>`) or error (`ErrorResponse`), across every
// module — reports timing without each handler needing to compute or thread
// it through manually.

use std::time::Instant;

use axum::{
    body::Body,
    extract::Request,
    http::header,
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Wraps the whole request/response cycle in a timer, then splices a
/// `processingTimeMs` field into the top-level JSON object of the response
/// body. Runs for every route, including 4xx/5xx error responses produced
/// via `AppError`, since it operates on the raw body rather than hooking
/// into the success-only `ApiResponse` envelope. Non-JSON responses (e.g.
/// Swagger UI's static assets) are passed through untouched.
pub async fn add_processing_time_to_body(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let response = next.run(request).await;

    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("application/json"));

    if !is_json {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        parts.headers.remove(header::CONTENT_LENGTH);
        return Response::from_parts(parts, Body::empty());
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;

    let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        parts.headers.remove(header::CONTENT_LENGTH);
        return Response::from_parts(parts, Body::from(bytes));
    };

    if let Some(object) = json.as_object_mut() {
        object.insert("processingTimeMs".to_string(), elapsed_ms.into());
    }

    let new_bytes = match serde_json::to_vec(&json) {
        Ok(bytes) => bytes,
        Err(_) => bytes.to_vec(),
    };

    parts.headers.remove(header::CONTENT_LENGTH);
    (parts, new_bytes).into_response()
}
