use serde::{Deserialize, Serialize};

/// One sticker group of a stored print job (snake_case like every stored field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelJobItemDocument {
    pub product_key: String,
    pub product_name: String,
    pub sku: String,
    pub serial_number: Option<String>,
    pub price_cents: Option<i64>,
    pub copies: u32,
}
