use axum::http::HeaderMap;

/// Id of the request being handled on this task, if any.
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok()
}

/// Accept a caller-supplied id only if it is short and plain, so a header
/// can't inject arbitrary text into every log line.
pub(super) fn incoming_request_id(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(REQUEST_ID_HEADER)?.to_str().ok()?.trim();
    let valid = !raw.is_empty()
        && raw.len() <= 128
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    valid.then(|| raw.to_string())
}

pub(super) fn is_textual(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|ct| {
        let ct = ct.to_ascii_lowercase();
        ct.starts_with("application/json")
            || ct.starts_with("text/")
            || ct.starts_with("application/x-www-form-urlencoded")
    })
}

tokio::task_local! {
    /// The current request's id, readable from anywhere in the handler's
    /// task — used to forward `X-Request-Id` on outbound calls.
    pub(super) static REQUEST_ID: String;
}

pub const REQUEST_ID_HEADER: &str = "x-request-id";
