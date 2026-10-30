use serde::Deserialize;
use utoipa::ToSchema;

/// The `staff{}` sub-object of `CreateSaleRequest` — just the cashier's
/// display name today, kept as its own struct (rather than a bare top-level
/// field) so the request groups "who completed this sale" the same way it
/// groups "who it was sold to" (`CustomerRef`) and "how it was paid"
/// (`PaymentDetails`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffRef {
    /// Display name of the staff member operating the register.
    pub cashier_name: String,
}
