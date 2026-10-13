// API-facing types for the quotations feature (priced offers given to a
// customer before — and independent of — any sale). No I/O and no Mongo
// types: the stored shape lives in `modules::quotations::model`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::billing::PricingAdjustments;

/// Lifecycle values stored in `status`. "Expired" is deliberately not one of
/// them: it is derived from `valid_until` at read time (`Quotation::is_expired`)
/// so an offer never needs a background job to flip it.
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_CANCELLED: &str = "cancelled";

/// One priced line of a quotation, embedded in the stored document and the
/// returned `Quotation`. A catalog line's `name`/`sku`/`unit_price_cents` are
/// resolved from the product at creation time and then frozen — a quotation
/// keeps reading the way it did when it was given to the customer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationItem {
    /// Catalog product this line was priced from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    /// Item description shown on the printed quotation.
    pub name: String,
    /// Stock Keeping Unit, if the line came from the catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Price per unit in cents.
    pub unit_price_cents: i64,
    /// Number of units offered.
    pub quantity: i64,
    /// Discount on this line in cents.
    pub discount_cents: i64,
    /// `unit_price_cents * quantity - discount_cents`, floored at 0.
    pub total_cents: i64,
}

/// The `customer{}` sub-object of `CreateQuotationRequest`. When `customerKey`
/// resolves to a real customer that record is authoritative for name, phone
/// and address; the typed values are only used for a walk-in with no key.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationCustomer {
    /// Registered customer key.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name (required when there is no `customerKey`).
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number.
    #[serde(default)]
    pub customer_phone: Option<String>,
    /// Walk-in customer address.
    #[serde(default)]
    pub customer_address: Option<String>,
}

/// One requested line. With a `productKey` the server takes name, SKU and
/// price from the catalog and ignores any client-sent values; without one the
/// line is ad hoc and `name` + `unitPriceCents` are required.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuotationItem {
    #[serde(default)]
    pub product_key: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub unit_price_cents: Option<i64>,
    /// Units offered; must be at least 1.
    pub quantity: i64,
    /// Discount on this line in cents (cannot exceed the line's gross value).
    #[serde(default)]
    pub discount_cents: i64,
}

/// Body for `POST /quotations`. Totals are never client-supplied — the server
/// recomputes every figure from the lines and `pricingAdjustments`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuotationRequest {
    #[serde(default)]
    pub customer: QuotationCustomer,
    pub items: Vec<CreateQuotationItem>,
    /// Order-level discount; omitted means none.
    #[serde(default)]
    pub pricing_adjustments: Option<PricingAdjustments>,
    /// Last day the offer holds, `YYYY-MM-DD`; today or later.
    pub valid_until: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// Terms and conditions printed on the quotation.
    #[serde(default)]
    pub terms: Option<String>,
    /// Name of the person who prepared it (display only).
    #[serde(default)]
    pub prepared_by: Option<String>,
}

/// A persisted quotation as returned by the API. The frozen shop-profile
/// snapshot is intentionally not part of it (it can carry a base64 logo);
/// only the PDF builder reads it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Quotation {
    pub id: String,
    pub key: String,
    /// Human-facing number, e.g. `QUO-0001`.
    pub quotation_number: String,
    /// `active` or `cancelled`.
    pub status: String,
    /// True when still `active` but `validUntil` is in the past.
    pub is_expired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address: Option<String>,
    pub items: Vec<QuotationItem>,
    pub subtotal_cents: i64,
    pub discount_type: String,
    pub discount_value: f64,
    pub discount_cents: i64,
    pub total_cents: i64,
    pub valid_until: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A quotation as it travels between devices and the cloud (`GET /sync/pull`,
/// snapshots, pushes). Unlike the REST `Quotation` it carries the frozen shop
/// profile (a device that applies a quotation must be able to reprint it
/// exactly) and the sync metadata, and it has no `isExpired`: that is derived
/// from the date at read time and would otherwise count as a content change.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncQuotation {
    pub id: String,
    pub key: String,
    pub quotation_number: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address: Option<String>,
    pub items: Vec<QuotationItem>,
    pub subtotal_cents: i64,
    pub discount_type: String,
    pub discount_value: f64,
    pub discount_cents: i64,
    pub total_cents: i64,
    pub valid_until: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_by: Option<String>,
    pub shop_profile_snapshot: serde_json::Value,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Query parameters for `GET /quotations`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct QuotationListQuery {
    /// Keyword matching quotation number or customer name/phone.
    pub search: Option<String>,
    /// `active` (still valid), `expired`, or `cancelled`. Omitted = all.
    pub status: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page size.
    pub limit: Option<u64>,
}

/// Paginated list payload for `GET /quotations`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationListResponse {
    pub quotations: Vec<Quotation>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
