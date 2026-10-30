use super::contact_person::ContactPerson;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /suppliers` and `PUT /suppliers/{id}` (PUT takes the same
/// full-representation shape as POST, per the frontend contract). `key` is
/// never client-supplied — generated server-side on create only, and never
/// touched by an update (see `service::create_supplier`/`replace_supplier`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSupplierRequest {
    /// Supplier name.
    pub name: String,
    /// Main contact person's name (optional if `contact_persons` is provided).
    #[serde(default)]
    pub contact_person: Option<String>,
    /// Primary phone number (optional if `contact_persons` is provided).
    #[serde(default)]
    pub primary_phone: Option<String>,
    /// Optional secondary phone number.
    #[serde(default)]
    pub secondary_phone: Option<String>,
    /// Contact persons with up to 2 phone numbers each.
    #[serde(default)]
    pub contact_persons: Option<Vec<ContactPerson>>,
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
