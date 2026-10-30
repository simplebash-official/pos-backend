use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /supplier-products`. Creates the link if
/// `(supplierKey, productKey)` has no existing link, otherwise updates
/// `costPriceCents`/`notes` on the existing one in place (see
/// `service::link::upsert_link`) rather than erroring on a duplicate.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpsertSupplierProductLinkRequest {
    /// Foreign key of the supplier to link.
    pub supplier_key: String,
    /// Foreign key of the product to link.
    pub product_key: String,
    /// Optional agreed cost price in cents.
    #[serde(default)]
    pub cost_price_cents: Option<i64>,
    /// Optional supplier notes.
    #[serde(default)]
    pub notes: Option<String>,
}
