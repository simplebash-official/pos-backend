use super::opening_stock_request::OpeningStockRequest;
use super::product_supplier_intake::ProductSupplierIntake;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /products`. `sku` is deliberately absent — it's
/// generated server-side from the category/subcategory names resolved from
/// `category_key`/`subcategory_key` (see `service::sku::generate_sku`), not
/// client-supplied. `category_key`/`subcategory_key` must be the system-
/// generated `key` of an existing category/subcategory (see `GET
/// /categories` or `GET /categories/valid`) — never the display name.
/// A product carries no price of its own: every unit it starts with comes in
/// as a stock batch with its own cost and selling price — `openingStock` for
/// units on hand, or one purchase per `suppliers` entry. A product created
/// with neither has no stock and no price until its first purchase.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductRequest {
    /// Optional manual barcode (omitted or null to skip).
    #[serde(default)]
    pub barcode: Option<String>,
    /// True to automatically generate a sequential EAN-13 barcode.
    #[serde(default)]
    pub auto_generate_barcode: bool,
    /// Name of the new product.
    pub name: String,
    /// Category key under which the product belongs.
    pub category_key: String,
    /// Subcategory key under which the product belongs.
    pub subcategory_key: String,
    /// Units on hand that did not come from a supplier purchase.
    #[serde(default)]
    pub opening_stock: Option<OpeningStockRequest>,
    /// Minimum stock alert threshold.
    #[serde(default)]
    pub min_stock_threshold: i64,
    /// Optional initial purchase deliveries from suppliers.
    #[serde(default)]
    pub suppliers: Vec<ProductSupplierIntake>,
    /// Whether this product's units are tracked individually by serial
    /// number. Once set, every purchase receipt and sale of this product
    /// must supply exact-count serial numbers (see `ProductSerial`).
    #[serde(default)]
    pub is_serialized: bool,
    /// Warranty length in months for units of this product, if any.
    #[serde(default)]
    pub warranty_months: Option<i64>,
}
