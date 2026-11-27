//! A frame the customer wants on a photo and framing job: what to make, never what to charge.

use super::print_frame_size::frame_width_inches;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One wanted frame, as stored on the job and returned to clients. The size and width are copied
/// from the listed frame size when it is picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobFrame {
    /// The listed frame size (`pfs_…`) it was picked from.
    pub frame_size_key: String,
    pub size: String,
    pub width_hundredths_inch: i64,
    pub quantity: i64,
}

impl PrintJobFrame {
    /// The frame as the customer's job receipt lists it.
    #[must_use]
    pub fn receipt_line(&self) -> String {
        format!(
            "Frame {} · {} inch x {}",
            self.size,
            frame_width_inches(self.width_hundredths_inch),
            self.quantity
        )
    }
}

/// A wanted frame as the print form sends it; the server reads the size and width from the list.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobFrameInput {
    pub frame_size_key: String,
    pub quantity: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_receipt_line_names_the_size_width_and_quantity() {
        let frame = PrintJobFrame {
            frame_size_key: "pfs_1".to_string(),
            size: "8x10".to_string(),
            width_hundredths_inch: 75,
            quantity: 2,
        };
        assert_eq!(frame.receipt_line(), "Frame 8x10 · 0.75 inch x 2");
    }
}
