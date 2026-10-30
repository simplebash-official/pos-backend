use super::local_writes::local_writes;

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

/// Sync-agent calls that never add anything to upload (or are the upload
/// itself); everything else that writes may have queued a change.
const NOT_LOCAL_CHANGES: &[&str] = &[
    "/api/sync/apply",
    "/api/sync/outbox/ack",
    "/api/sync/state",
    "/api/sync/blocks",
    "/api/logs",
    // Live calculations store nothing.
    "/api/calc",
];

/// Middleware: after every successful write request, tell the local event
/// stream. Writes that queued nothing cost the agent one empty outbox read.
pub async fn local_write_tick(request: Request, next: Next) -> Response {
    let writes = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) && !NOT_LOCAL_CHANGES
        .iter()
        .any(|p| request.uri().path().starts_with(p));
    let response = next.run(request).await;
    if writes && response.status().is_success() {
        local_writes().send_modify(|n| *n = n.wrapping_add(1));
    }
    response
}
