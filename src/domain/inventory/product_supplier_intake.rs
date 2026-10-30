use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// An individual supplier intake batch submitted during product creation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSupplierIntake {
    /// Key of the supplier providing this stock.
    pub supplier_key: String,
    /// Number of units supplied.
    pub quantity: i64,
    /// Purchase unit cost in cents.
    pub cost_price_cents: i64,
    /// Price the units of this intake sell at, in cents.
    pub selling_price_cents: i64,
    /// Supplier invoice or shipment reference number.
    #[serde(default)]
    pub reference_no: Option<String>,
    /// Optional remarks or notes for this intake.
    #[serde(default)]
    pub notes: Option<String>,
}
