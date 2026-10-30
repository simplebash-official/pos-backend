use serde::Deserialize;
use utoipa::ToSchema;

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
