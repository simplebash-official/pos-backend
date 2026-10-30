use super::sticker::Sticker;
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
