// Pure business types for the supplier purchase / stock-intake history
// feature — no I/O, no Mongo/Axum types beyond serde/utoipa derives.

use crate::domain::inventory::PaginationMeta;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Minimal supplier display data embedded on a `Purchase`, resolved live at
/// read time from `suppliers::service::get_supplier_by_key` — absent
/// (rather than a fabricated placeholder) if the supplier no longer exists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierSummary {
    /// MongoDB hex ID.
    pub id: String,
    /// Supplier business key.
    pub key: String,
    /// Supplier company or individual name.
    pub name: String,
    /// Contact person's name.
    pub contact_person: String,
    /// Primary contact phone number.
    pub primary_phone: String,
}

/// Minimal product display data embedded on a `Purchase`, resolved live at
/// read time from `inventory::service::product::get_product_by_key` —
/// absent if the product no longer exists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    /// MongoDB hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// SKU identifier code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Parent category display name.
    pub category: String,
    /// Subcategory display name.
    pub subcategory: String,
}

/// A recorded stock intake as returned to API clients. `totalCostCents` is
/// computed at read time (`quantity * unitCostCents`), never stored — this
/// codebase never persists a derived numeric field that could go stale
/// (compare `LowStockItem.deficit`, also computed on the way out).
/// `supplier`/`product` are the enrichment the frontend's mock layer
/// currently builds client-side from separate calls.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this purchase (e.g. pur_...).
    pub key: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// Foreign key referencing the product.
    pub product_key: String,
    /// Number of units purchased and received.
    pub quantity: i64,
    /// Unit cost in cents.
    pub unit_cost_cents: i64,
    /// Calculated total cost in cents (quantity * unit_cost_cents).
    pub total_cost_cents: i64,
    /// Purchase receipt date.
    pub date: DateTime<Utc>,
    /// Supplier invoice or order reference number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    /// Optional remarks or notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Enriched supplier details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierSummary>,
    /// Enriched product details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<ProductSummary>,
    /// Timestamp when purchase was recorded.
    pub created_at: DateTime<Utc>,
    /// Timestamp when purchase was last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Soft deletion timestamp if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /purchases`. Recording a purchase also increments the
/// referenced product's `stockQuantity` and writes a `PurchaseReceipt`
/// stock movement (see `service::purchase::record_purchase`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePurchaseRequest {
    /// Foreign key of the supplier.
    pub supplier_key: String,
    /// Foreign key of the product.
    pub product_key: String,
    /// Quantity of items received.
    pub quantity: i64,
    /// Cost price per item in cents.
    pub unit_cost_cents: i64,
    /// Date when the purchase was received.
    pub date: DateTime<Utc>,
    /// Optional shipment or invoice reference number.
    #[serde(default)]
    pub reference_no: Option<String>,
    /// Optional remarks.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Paginated response for `GET /purchases`. `items` and `purchases` carry the
/// same rows under both names — `purchases` is the shape the frontend already
/// reads, `items` the generic one the sync client expects.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseListResponse {
    /// List of purchase history records.
    pub items: Vec<Purchase>,
    /// Duplicate list alias for backwards-compatibility.
    pub purchases: Vec<Purchase>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}

/// Query params for `GET /purchases`. `supplierKey` and `productKey` are optional
/// to support full collection syncs.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PurchaseListQuery {
    /// Filter purchases by supplier key.
    pub supplier_key: Option<String>,
    /// Filter purchases by product key.
    pub product_key: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}
