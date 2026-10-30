use super::decoded_cursor::DecodedCursor;
use super::tombstone_retention_days::TOMBSTONE_RETENTION_DAYS;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};

pub fn decode_cursor(cursor_str: &str) -> AppResult<DecodedCursor> {
    if cursor_str.trim().is_empty() {
        return Err(AppError::validation_with_code(
            "Empty cursor string",
            codes::CURSOR_INVALID,
        ));
    }

    // Standard base64 is still accepted so cursors minted before the switch
    // to the URL-safe alphabet keep working instead of forcing every client
    // into a full refresh on the deploy that introduced it.
    let trimmed = cursor_str.trim();
    let bytes = URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| STANDARD.decode(trimmed))
        .map_err(|_| {
            AppError::validation_with_code("Invalid base64 cursor", codes::CURSOR_INVALID)
        })?;

    let decoded_str = String::from_utf8(bytes).map_err(|_| {
        AppError::validation_with_code("Invalid cursor encoding", codes::CURSOR_INVALID)
    })?;

    let parts: Vec<&str> = decoded_str.split('|').collect();
    if parts.is_empty() {
        return Err(AppError::validation_with_code(
            "Malformed cursor payload",
            codes::CURSOR_INVALID,
        ));
    }

    let millis: i64 = parts[0].parse().map_err(|_| {
        AppError::validation_with_code("Invalid cursor timestamp", codes::CURSOR_INVALID)
    })?;

    let timestamp = DateTime::from_timestamp_millis(millis).ok_or_else(|| {
        AppError::validation_with_code("Timestamp out of range", codes::CURSOR_INVALID)
    })?;

    let key = if parts.len() > 1 && !parts[1].is_empty() {
        Some(parts[1].to_string())
    } else {
        None
    };

    // 90-day retention validation
    let cutoff = Utc::now() - Duration::days(TOMBSTONE_RETENTION_DAYS);
    if timestamp < cutoff {
        return Err(AppError::validation_with_code(
            format!(
                "Cursor timestamp is older than the {TOMBSTONE_RETENTION_DAYS}-day tombstone retention window"
            ),
            codes::CURSOR_INVALID,
        ));
    }

    Ok(DecodedCursor { timestamp, key })
}
