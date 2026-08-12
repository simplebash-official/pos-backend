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
    pub id: String,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_person: Option<String>,
    pub primary_phone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub outstanding_balance_cents: i64,
    pub total_purchases_cents: i64,
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

/// Body for `POST /customers` and `PUT /customers/{id}` (PUT takes the same
/// full-representation profile shape as POST). `id`, `key`, financial totals,
/// and timestamps are strictly omitted and maintained by the backend.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCustomerRequest {
    pub name: String,
    #[serde(default)]
    pub contact_person: Option<String>,
    pub primary_phone: String,
    #[serde(default)]
    pub secondary_phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Body for `PATCH /customers/{id}`. Every field is optional so a client sends
/// only what changed — `service::update_customer` fills in omitted fields
/// from the existing document rather than clearing them.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCustomerRequest {
    pub name: Option<String>,
    pub contact_person: Option<String>,
    pub primary_phone: Option<String>,
    pub secondary_phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
}

/// Query params for `GET /customers`. `search` matches against name,
/// phone numbers, contact person, email, and address; `tag` filters by
/// membership in `tags`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CustomerListQuery {
    pub search: Option<String>,
    pub tag: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// Response for `GET /customers` matching frontend paginated structure.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerListResponse {
    pub customers: Vec<Customer>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// Body for `DELETE /customers/batch`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteCustomersRequest {
    pub ids: Vec<String>,
}

/// Response for `DELETE /customers/batch`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCustomersResponse {
    pub deleted_count: u64,
}

/// Response for `GET /customers/tags` — every distinct tag currently in use
/// across all customers' `tags` arrays, for client-side autocompletion.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerTagsResponse {
    pub tags: Vec<String>,
}
