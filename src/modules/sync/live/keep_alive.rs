use super::heartbeat::HEARTBEAT;

use axum::response::sse::KeepAlive;

/// SSE keep-alive used by both channels.
pub fn keep_alive() -> KeepAlive {
    KeepAlive::new().interval(HEARTBEAT).text("ping")
}
