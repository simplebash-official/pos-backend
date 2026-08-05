// Pure business types for the supplier purchase / stock-intake history
// feature — no I/O, no Mongo/Axum types beyond serde/utoipa derives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Minimal supplier display data embedded on a `Purchase`, resolved live at
/// read time from `suppliers::service::get_supplier_by_key` — absent
/// (rather than a fabricated placeholder) if the supplier no longer exists.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierSummary {
    pub id: String,
    pub key: String,
    pub name: String,
    pub contact_person: String,
    pub primary_phone: String,
}

/// Minimal product display data embedded on a `Purchase`, resolved live at
/// read time from `inventory::service::product::get_product_by_key` —
/// absent if the product no longer exists.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    pub id: String,
    pub key: String,
    pub sku: String,
    pub name: String,
    pub category: String,
    pub subcategory: String,
}

/// A recorded stock intake as returned to API clients. `totalCostCents` is
/// computed at read time (`quantity * unitCostCents`), never stored — this
/// codebase never persists a derived numeric field that could go stale
/// (compare `LowStockItem.deficit`, also computed on the way out).
/// `supplier`/`product` are the enrichment the frontend's mock layer
/// currently builds client-side from separate calls.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    pub id: String,
    pub key: String,
    pub supplier_key: String,
    pub product_key: String,
    pub quantity: i64,
    pub unit_cost_cents: i64,
    pub total_cost_cents: i64,
    pub date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<ProductSummary>,
}

/// Body for `POST /purchases`. Recording a purchase also increments the
/// referenced product's `stockQuantity` and writes a `PurchaseReceipt`
/// stock movement (see `service::purchase::record_purchase`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePurchaseRequest {
    pub supplier_key: String,
    pub product_key: String,
    pub quantity: i64,
    pub unit_cost_cents: i64,
    pub date: DateTime<Utc>,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Response for `GET /purchases`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchasesResponse {
    pub purchases: Vec<Purchase>,
}

/// Query params for `GET /purchases`. At least one of `supplierKey`/
/// `productKey` must be given (see `service::purchase::list_purchases`).
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PurchaseListQuery {
    pub supplier_key: Option<String>,
    pub product_key: Option<String>,
}
