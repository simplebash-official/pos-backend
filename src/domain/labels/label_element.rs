use super::label_frame::LabelFrame;
use super::text_align::TextAlign;
use super::text_source::TextSource;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One thing drawn on a label. Positions and sizes are millimetres from the
/// label's top-left corner. Every field is required (no implicit defaults);
/// `custom_text` is `null` for every source except `customText`. This is also
/// the exact shape the document-server template's schema accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LabelElement {
    #[serde(rename_all = "camelCase")]
    Text {
        id: String,
        source: TextSource,
        custom_text: Option<String>,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
        font_size_pt: f64,
        bold: bool,
        align: TextAlign,
    },
    /// A Code128 barcode of the label's code; `show_value` prints it underneath.
    #[serde(rename_all = "camelCase")]
    Barcode {
        id: String,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
        show_value: bool,
        font_size_pt: f64,
    },
    /// A QR code of the label's code, drawn as a square in the box.
    #[serde(rename_all = "camelCase")]
    Qr {
        id: String,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
    },
    /// A solid black rectangle, used as a rule or divider.
    #[serde(rename_all = "camelCase")]
    Line {
        id: String,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
    },
}

impl LabelElement {
    /// Geometry shared by every element kind.
    #[must_use]
    pub fn frame(&self) -> LabelFrame<'_> {
        match self {
            Self::Text {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                ..
            }
            | Self::Barcode {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                ..
            }
            | Self::Qr {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
            }
            | Self::Line {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
            } => LabelFrame {
                id,
                x_mm: *x_mm,
                y_mm: *y_mm,
                width_mm: *width_mm,
                height_mm: *height_mm,
            },
        }
    }
}
