use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A subcategory as returned to API clients, persisted in its own
/// `subcategories` collection (see `modules::inventory::model::SubcategoryDocument`).
/// `category_key` is the FK back to its parent `CategoryInfo.key` — a
/// subcategory always belongs to exactly one category.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoryInfo {
    /// Unique business key identifying the subcategory (e.g. sub_...).
    pub key: String,
    /// Foreign key referencing the parent category key.
    pub category_key: String,
    /// Subcategory display name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Version counter.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this subcategory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Body for `PUT /categories/{categoryKey}` (admin-only). All fields
/// optional, same partial-update convention as `UpdateProductRequest`.
/// Renaming (`name` set to a new value) is a pure display-label change —
/// products reference a category by its immutable `key`, so no cascade is
/// needed (see `service::category::update_category`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    /// Updated display name.
    pub name: Option<String>,
    /// Updated icon name.
    pub icon: Option<String>,
    /// Updated theme color code.
    pub color: Option<String>,
}

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

/// Body for `PATCH /batches/{key}`. Either field may be sent alone. Changing
/// the cost is Admin-only (the handler checks it).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStockBatchRequest {
    /// New selling price for the batch's remaining units, in cents.
    pub selling_price_cents: Option<i64>,
    /// Corrected unit cost, in cents (Admin only).
    pub unit_cost_cents: Option<i64>,
}

/// Response for `GET /categories/valid` — the same category/subcategory
/// data as `CategoriesResponse`, reshaped as key+name pairs for callers
/// (like a product form) that need to submit `categoryKey`/`subcategoryKey`
/// while displaying the human-readable name.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoriesResponse {
    /// List of category and subcategory selection options.
    pub categories: Vec<ValidCategoryOption>,
}

/// One category option within `ValidCategoriesResponse` — key + display
/// name plus its own nested subcategory options, so a client (e.g. a
/// product-creation form) can render names while submitting keys.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoryOption {
    /// Category key.
    pub key: String,
    /// Category name.
    pub name: String,
    /// Subcategory selection options.
    pub subcategories: Vec<ValidSubcategoryOption>,
}

/// One subcategory option nested under `ValidCategoryOption`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidSubcategoryOption {
    /// Subcategory key.
    pub key: String,
    /// Subcategory name.
    pub name: String,
}
