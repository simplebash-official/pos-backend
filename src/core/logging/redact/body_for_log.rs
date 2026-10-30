use super::redact_json_text::redact_json_text;
use super::redact_text::redact_text;
use super::redact_value::redact_value;

use serde_json::Value;

/// Bodies larger than this multiple of the cap are never parsed: only their
/// first `cap` bytes are scrubbed as text, so logging a multi-MB body costs
/// O(cap) instead of a full parse + re-serialize.
const PARSE_LIMIT_FACTOR: usize = 4;

/// Render a captured body for the log: JSON is redacted and re-serialized,
/// other text is secret-scrubbed; both are capped at `cap` bytes.
#[must_use]
pub fn body_for_log(bytes: &[u8], cap: usize) -> String {
    if bytes.len() > cap.saturating_mul(PARSE_LIMIT_FACTOR) {
        let mut end = cap.min(bytes.len());
        // Don't split a UTF-8 sequence.
        while end > 0 && end < bytes.len() && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
            end -= 1;
        }
        let prefix = String::from_utf8_lossy(&bytes[..end]);
        return format!(
            "{}…[truncated, {} bytes total]",
            redact_json_text(&prefix),
            bytes.len()
        );
    }
    let rendered = match serde_json::from_slice::<Value>(bytes) {
        Ok(mut json) => {
            redact_value(&mut json);
            json.to_string()
        }
        Err(_) => redact_text(&String::from_utf8_lossy(bytes)),
    };
    truncate(rendered, cap)
}

fn truncate(mut s: String, cap: usize) -> String {
    if s.len() <= cap {
        return s;
    }
    let mut cut = cap;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    s.push_str("…[truncated]");
    s
}
