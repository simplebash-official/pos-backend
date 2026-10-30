use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
