use serde::Deserialize;
use utoipa::ToSchema;

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
