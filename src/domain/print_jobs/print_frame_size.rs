//! A shop's frame size list: each size ("8x10", "A4") with a frame width. It records what a
//! customer can ask for on a photo and framing job. It holds no price: charging is done with the
//! job's items.

use crate::domain::cash_categories::normalize_name;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// The form of a size two entries are compared by: lowercase, without spaces, with `×` read as
/// `x`, so "8 X 10", "8×10" and "8x10" are one size.
#[must_use]
pub fn normalize_frame_size(size: &str) -> String {
    normalize_name(size).replace('×', "x")
}

/// The key of this size and width: `pfs_` and the first 12 hex digits of the SHA-256 of both.
/// Derived, not random, so the same frame added on two offline tills is one row once they sync
/// (the rule print services follow).
#[must_use]
pub fn print_frame_size_key(size: &str, width_hundredths_inch: i64) -> String {
    use sha2::{Digest, Sha256};
    let identity = format!("{}|{width_hundredths_inch}", normalize_frame_size(size));
    let digest = Sha256::digest(identity.as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{hex}",
        crate::core::constants::prefixes::PRINT_FRAME_SIZE
    )
}

/// A frame width in inches as the shop writes it: 75 hundredths is "0.75", 100 is "1".
#[must_use]
pub fn frame_width_inches(width_hundredths_inch: i64) -> String {
    format!("{}", width_hundredths_inch as f64 / 100.0)
}

/// A frame size as returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintFrameSize {
    pub id: String,
    /// `pfs_…`.
    pub key: String,
    /// The frame size as the shop writes it. Never changes.
    pub size: String,
    /// The width of the frame itself, in hundredths of an inch (75 is 0.75 inch). Never changes.
    pub width_hundredths_inch: i64,
    /// A hidden size stays on old jobs but is not offered on new ones.
    pub active: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Body of `POST /print-frame-sizes`. A new size is offered.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrintFrameSizeRequest {
    pub size: String,
    pub width_hundredths_inch: i64,
}

/// Body of `PUT /print-frame-sizes/{key}`: whether the size is offered is all that can change.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePrintFrameSizeRequest {
    pub active: bool,
}

/// Query of `GET /print-frame-sizes`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PrintFrameSizeListQuery {
    /// Also list hidden sizes (the settings screen); the print form leaves it out.
    pub include_hidden: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_typed_another_way_is_the_same_frame() {
        let key = print_frame_size_key("8x10", 75);
        for typed in ["8 X 10", "8×10", " 8x10 "] {
            assert_eq!(print_frame_size_key(typed, 75), key, "{typed}");
        }
        assert_eq!(normalize_frame_size("A4"), normalize_frame_size("a 4"));
    }

    #[test]
    fn another_size_or_width_is_another_frame() {
        let key = print_frame_size_key("8x10", 75);
        assert_ne!(print_frame_size_key("8x10", 100), key);
        assert_ne!(print_frame_size_key("8x12", 75), key);
    }

    #[test]
    fn the_key_is_stable_and_short() {
        let key = print_frame_size_key("8x10", 75);
        assert!(key.starts_with("pfs_"), "{key}");
        assert_eq!(key.len(), "pfs_".len() + 12);
    }

    #[test]
    fn a_width_is_written_without_needless_zeros() {
        assert_eq!(frame_width_inches(75), "0.75");
        assert_eq!(frame_width_inches(100), "1");
        assert_eq!(frame_width_inches(115), "1.15");
        assert_eq!(frame_width_inches(10_000), "100");
    }
}
