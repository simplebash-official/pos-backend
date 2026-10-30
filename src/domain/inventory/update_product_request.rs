use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PUT /products/{id}`. Every field is optional so a client can
/// send only what changed — `service::product::update_product` fills in
/// omitted fields from the existing document rather than clearing them.
/// `category_key`/`subcategory_key`, like on create, must be keys, not names.
/// `sku` stays immutable (no field here); `barcode` can be set/corrected once,
/// so a product created without the right barcode (or with an auto-generated
/// one) can be pointed at the real label number later.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProductRequest {
    /// Updated product name.
    pub name: Option<String>,
    /// Add or correct the scannable barcode (e.g. store the manufacturer
    /// barcode printed on the package). 8-14 digits; saved as a manual
    /// barcode. Omit to leave the existing barcode untouched — an empty
    /// string is a validation error, not a "clear".
    pub barcode: Option<String>,
    /// Updated category key.
    pub category_key: Option<String>,
    /// Updated subcategory key.
    pub subcategory_key: Option<String>,
    /// Updated low-stock warning threshold.
    pub min_stock_threshold: Option<i64>,
    /// Updated serialized-tracking flag.
    pub is_serialized: Option<bool>,
    /// Updated warranty length in months.
    pub warranty_months: Option<i64>,
}
