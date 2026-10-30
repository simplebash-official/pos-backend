use super::request_id::REQUEST_ID;

/// Id of the request being handled on this task, if any.
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok()
}
