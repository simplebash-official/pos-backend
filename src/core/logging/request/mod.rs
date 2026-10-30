// Outermost middleware: gives every request a `request_id` (taken from the
// caller's `X-Request-Id` so a frontend click, this request and the
// document-server render it triggers share one id), opens a `request` span
// that every log line inside the handler inherits (`request_id`, `user_id`),
// and logs `http/request` + `http/response` with status, latency, sizes and
// — when `LOG_HTTP_BODIES` is on — the redacted, capped bodies.

mod current_request_id;
mod incoming_request_id;
mod is_textual;
mod log_requests;
mod request_id;
mod request_id_header;
#[cfg(test)]
mod tests;

pub use current_request_id::current_request_id;
pub use log_requests::log_requests;
pub use request_id_header::REQUEST_ID_HEADER;
