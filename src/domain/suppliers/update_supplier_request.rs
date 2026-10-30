use super::contact_person::ContactPerson;

use serde::Deserialize;
use utoipa::ToSchema;

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
    /// Updated contact persons list.
    pub contact_persons: Option<Vec<ContactPerson>>,
    /// Updated address.
    pub address: Option<String>,
    /// Updated supplied categories list.
    pub supplied_categories: Option<Vec<String>>,
    /// Updated email address.
    pub email: Option<String>,
    /// Updated remarks or notes.
    pub notes: Option<String>,
}
