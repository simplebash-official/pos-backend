use super::request_id_header::REQUEST_ID_HEADER;

use axum::http::HeaderMap;

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
