#![forbid(unsafe_code)]

/// Maximum accepted clock lead of a sender over the receiver.
const MAX_FUTURE_MS: i64 = 5 * 60 * 1000;

/// Future timestamps beyond `now + 5 min` are clamped to `now`; anything else
/// (including the past) is taken as sent. Returns `(effective_ms, clamped)`.
#[must_use]
pub fn clamp_updated_at(incoming_ms: i64, server_now_ms: i64) -> (i64, bool) {
    if incoming_ms > server_now_ms.saturating_add(MAX_FUTURE_MS) {
        (server_now_ms, true)
    } else {
        (incoming_ms, false)
    }
}
