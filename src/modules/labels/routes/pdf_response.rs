use axum::body::Body;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

pub(super) fn pdf_response(bytes: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(bytes),
    )
        .into_response()
}
