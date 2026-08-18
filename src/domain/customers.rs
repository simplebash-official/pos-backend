// Pure business types for the customers feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::customers::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A customer as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key; `key` is the human-shareable
/// prefixed id (see `core::id::generate_id`), used by other modules (`billing`,
/// `repairs`, etc.) to reference this customer without holding its `ObjectId`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (e.g. cus_...).
    pub key: String,
    /// Full customer name or business name.
    pub name: String,
    /// Point of contact person for organizations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_person: Option<String>,
    /// Primary contact phone number.
    pub primary_phone: String,
    /// Secondary contact phone number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    /// Email address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Physical mailing or street address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// List of assigned customer category tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Freeform internal customer notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Outstanding debt/credit balance in cents.
    pub outstanding_balance_cents: i64,
    /// Total money spent across all purchases in cents.
    pub total_purchases_cents: i64,
    /// Timestamp when created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp of deletion if customer is soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier of client that last modified this customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /customers` and `PUT /customers/{id}` (PUT takes the same
/// full-representation profile shape as POST). `id`, `key`, financial totals,
/// and timestamps are strictly omitted and maintained by the backend.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCustomerRequest {
    /// Full customer name.
    pub name: String,
    /// Optional contact person.
    #[serde(default)]
    pub contact_person: Option<String>,
    /// Primary phone number.
    pub primary_phone: String,
    /// Optional secondary phone number.
    #[serde(default)]
    pub secondary_phone: Option<String>,
    /// Optional email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Optional street address.
    #[serde(default)]
    pub address: Option<String>,
    /// Optional classification tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional internal notes.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Body for `PATCH /customers/{id}`. Every field is optional so a client sends
/// only what changed — `service::update_customer` fills in omitted fields
/// from the existing document rather than clearing them.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCustomerRequest {
    /// Updated customer name.
    pub name: Option<String>,
    /// Updated contact person.
    pub contact_person: Option<String>,
    /// Updated primary phone number.
    pub primary_phone: Option<String>,
    /// Updated secondary phone number.
    pub secondary_phone: Option<String>,
    /// Updated email address.
    pub email: Option<String>,
    /// Updated street address.
    pub address: Option<String>,
    /// Updated classification tags.
    pub tags: Option<Vec<String>>,
    /// Updated internal notes.
    pub notes: Option<String>,
}

/// Query params for `GET /customers`. `search` matches against name,
/// phone numbers, contact person, email, and address; `tag` filters by
/// membership in `tags`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CustomerListQuery {
    /// Keyword search query.
    pub search: Option<String>,
    /// Tag filter (exact match within tags array).
    pub tag: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Maximum items per page.
    pub limit: Option<u64>,
    /// Field name to sort by.
    pub sort_by: Option<String>,
    /// Sort direction ("asc" or "desc").
    pub sort_order: Option<String>,
}

/// Response for `GET /customers` matching frontend paginated structure.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerListResponse {
    /// List of customer records on the current page.
    pub customers: Vec<Customer>,
    /// Total count of matching customer records.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Number of items per page.
    pub limit: u64,
    /// Total number of pages available.
    pub total_pages: u64,
}

/// Body for `DELETE /customers/batch`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteCustomersRequest {
    /// List of customer IDs or keys to delete.
    pub ids: Vec<String>,
}

/// Response for `DELETE /customers/batch`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCustomersResponse {
    /// Number of customer records deleted.
    pub deleted_count: u64,
}

/// Response for `GET /customers/tags` — every distinct tag currently in use
/// across all customers' `tags` arrays, for client-side autocompletion.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerTagsResponse {
    /// List of distinct customer tags.
    pub tags: Vec<String>,
}
