use super::incoming_request_id::incoming_request_id;
use super::is_textual::is_textual;
use super::request_id_header::REQUEST_ID_HEADER;

use axum::http::{HeaderMap, HeaderValue};

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
