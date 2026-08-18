// Pure business types for the suppliers feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::suppliers::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A supplier as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key (`GET/PUT/PATCH/DELETE
/// /suppliers/{id}`); `key` is the human-shareable prefixed id (see
/// `core::id::generate_id`), used by other modules (`supplier_products`,
/// `purchases`) to reference this supplier without holding its `ObjectId`.
/// `supplied_categories` are free-text tags (e.g. "Phone Parts") describing
/// what a supplier deals in — unlike `category_key` on a product, these are
/// not a foreign key into any collection, just descriptive labels a
/// supplier's own record carries.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Supplier {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this supplier (e.g. sup_...).
    pub key: String,
    /// Supplier company or vendor name.
    pub name: String,
    /// Primary contact person.
    pub contact_person: String,
    /// Primary telephone number.
    pub primary_phone: String,
    /// Secondary phone number if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    /// Physical or postal address.
    pub address: String,
    /// Categories of products supplied by this vendor.
    pub supplied_categories: Vec<String>,
    /// Email address of the supplier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// General notes or remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Timestamp when supplier was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when supplier was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /suppliers` and `PUT /suppliers/{id}` (PUT takes the same
/// full-representation shape as POST, per the frontend contract). `key` is
/// never client-supplied — generated server-side on create only, and never
/// touched by an update (see `service::create_supplier`/`replace_supplier`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSupplierRequest {
    /// Supplier name.
    pub name: String,
    /// Main contact person's name.
    pub contact_person: String,
    /// Primary phone number.
    pub primary_phone: String,
    /// Optional secondary phone number.
    #[serde(default)]
    pub secondary_phone: Option<String>,
    /// Address of supplier.
    pub address: String,
    /// List of supplied product categories.
    pub supplied_categories: Vec<String>,
    /// Optional email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Optional remarks or notes.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Body for `PATCH /suppliers/{id}`. Every field optional so a client sends
/// only what changed — `service::update_supplier` fills in omitted fields
/// from the existing document rather than clearing them (same convention as
/// `inventory`'s `UpdateProductRequest`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSupplierRequest {
    /// Updated supplier name.
    pub name: Option<String>,
    /// Updated contact person.
    pub contact_person: Option<String>,
    /// Updated primary phone.
    pub primary_phone: Option<String>,
    /// Updated secondary phone.
    pub secondary_phone: Option<String>,
    /// Updated address.
    pub address: Option<String>,
    /// Updated supplied categories list.
    pub supplied_categories: Option<Vec<String>>,
    /// Updated email address.
    pub email: Option<String>,
    /// Updated remarks or notes.
    pub notes: Option<String>,
}

/// Query params for `GET /suppliers`. `search` matches against name,
/// contact person, phone, and address; `category` filters by one of
/// `supplied_categories`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SupplierListQuery {
    /// Search term matching name, contact person, phone, or address.
    pub search: Option<String>,
    /// Filter suppliers by supplied category name.
    pub category: Option<String>,
}

/// Response for `GET /suppliers`. Unlike `inventory`'s product list, this
/// isn't paginated — a shop's supplier list is small enough to return in
/// full, matching the frontend contract (no page/limit params in the spec).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuppliersResponse {
    /// Complete list of suppliers.
    pub suppliers: Vec<Supplier>,
}

/// Body for `DELETE /suppliers/batch`. Ids that fail to delete (invalid
/// `ObjectId`, or blocked by the `SUPPLIER_HAS_PURCHASES` guard) are
/// silently skipped rather than failing the whole request — see
/// `service::delete_suppliers`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteSuppliersRequest {
    /// List of supplier Mongo hex IDs to delete.
    pub ids: Vec<String>,
}

/// Response for the batch-delete endpoint — count only, since the caller
/// already knows which ids it asked to delete.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSuppliersResponse {
    /// Number of suppliers successfully deleted.
    pub deleted_count: u64,
}

/// Response for `GET /suppliers/categories` — every distinct tag currently
/// in use across all suppliers' `supplied_categories`, for a client-side
/// autocomplete (replacing the frontend's hardcoded suggestion list).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierCategoriesResponse {
    /// Unique list of category tags across all suppliers.
    pub categories: Vec<String>,
}
