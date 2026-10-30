use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One sticker group of a print job, as it was printed (a snapshot: renaming
/// the product later does not change history). `serial_number` is set for a
/// serial label.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelJobItem {
    pub product_key: String,
    pub product_name: String,
    pub sku: String,
    pub serial_number: Option<String>,
    pub price_cents: Option<i64>,
    pub copies: u32,
}
