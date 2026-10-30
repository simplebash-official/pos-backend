use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};

/// URL-safe, unpadded — cursors travel in a query string, and standard
/// base64's `+` is decoded as a space by every form-urlencoded parser
/// (including the one behind axum's `Query`), which would silently corrupt
/// the cursor and force a full refresh on roughly half of all requests.
/// `/` and the `=` padding are likewise avoidable footguns here.
#[must_use]
pub fn encode_cursor(timestamp: DateTime<Utc>, key: &str) -> String {
    let raw = format!("{}|{}", timestamp.timestamp_millis(), key);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}
