use serde::Deserialize;
use utoipa::ToSchema;

/// The `customer{}` sub-object of `CreateRepairRequest`/`UpdateRepairRequest`.
/// `customerName`/`customerPhone` are only used as-is when `customerKey` is
/// absent (a walk-in with no linked account); when `customerKey` resolves
/// to a real customer, `service::create_repair`/`update_repair` overrides
/// both from that record instead of trusting the payload. `customerName`/
/// `customerPhone` are typed `Option` here (rather than required strings)
/// so the same struct serves both create and update — `service::
/// create_repair`'s `validate_required_fields` is what actually enforces
/// they're non-empty on create when there's no `customerKey` to resolve
/// from instead.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairCustomer {
    /// Key of registered customer.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name.
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number.
    #[serde(default)]
    pub customer_phone: Option<String>,
}
