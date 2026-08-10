// Pure business types for the supplier-product linking feature (the
// many-to-many join between `suppliers` and `inventory`'s products) — no
// I/O, no Mongo/Axum types beyond serde/utoipa derives.

use crate::domain::inventory::PaginationMeta;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A supplier-product link as returned to API clients. `supplierKey`/
/// `productKey` are the FKs this link joins — always a document `key`, per
/// this codebase's cross-reference convention, never a raw `ObjectId`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierProductLink {
    pub key: String,
    pub supplier_key: String,
    pub product_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_price_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub added_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /supplier-products`. Creates the link if
/// `(supplierKey, productKey)` has no existing link, otherwise updates
/// `costPriceCents`/`notes` on the existing one in place (see
/// `service::link::upsert_link`) rather than erroring on a duplicate.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpsertSupplierProductLinkRequest {
    pub supplier_key: String,
    pub product_key: String,
    #[serde(default)]
    pub cost_price_cents: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Body for `PUT /supplier-products/bulk/{supplierKey}` — the full set of
/// product keys a supplier should be linked to after this call. Any
/// existing link not in this list is removed; any new one is added; ones
/// that survive keep their existing `costPriceCents`/`notes` (see
/// `service::link::replace_links_for_supplier`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BulkReplaceLinksRequest {
    pub product_keys: Vec<String>,
}

/// Legacy/Array response for `GET /supplier-products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierProductLinksResponse {
    pub links: Vec<SupplierProductLink>,
}

/// Paginated response for `GET /supplier-products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierProductListResponse {
    pub items: Vec<SupplierProductLink>,
    pub links: Vec<SupplierProductLink>,
    pub pagination: PaginationMeta,
}

/// Query params for `GET /supplier-products`. Both `supplierKey` and `productKey`
/// are optional to support full collection syncs.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SupplierProductLinkQuery {
    pub supplier_key: Option<String>,
    pub product_key: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
