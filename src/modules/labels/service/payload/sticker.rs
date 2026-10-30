use serde::Serialize;

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
