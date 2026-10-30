use crate::core::error::{AppError, AppResult};
use crate::domain::labels::LabelElement;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LabelPayload<'a> {
    width_mm: f64,
    height_mm: f64,
    shop_name: &'a str,
    elements: &'a [LabelElement],
    labels: &'a [Sticker],
}

/// The template's `sys.inputs`: the layout plus the stickers to draw with it.
pub(crate) fn build_label_data(
    width_mm: f64,
    height_mm: f64,
    shop_name: &str,
    elements: &[LabelElement],
    stickers: &[Sticker],
) -> AppResult<Value> {
    serde_json::to_value(LabelPayload {
        width_mm,
        height_mm,
        shop_name,
        elements,
        labels: stickers,
    })
    .map_err(|e| AppError::internal(format!("Failed to build the label payload: {e}")))
}

/// One sticker to print, printed `copies` times. `price_cents` is `None` only
/// when the design has no price element.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sticker {
    /// Which product this sticker is for; kept for the print history, never
    /// sent to document-server.
    #[serde(skip)]
    pub product_key: String,
    /// The unit's serial number on a serial label (history only, not sent).
    #[serde(skip)]
    pub serial_number: Option<String>,
    pub name: String,
    pub sku: String,
    pub category: String,
    pub price_cents: Option<i64>,
    pub code: String,
    pub copies: u32,
}
