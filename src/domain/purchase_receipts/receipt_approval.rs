use serde::Deserialize;
use utoipa::ToSchema;

/// A manager or owner's approval for the cash this receipt takes out of the drawer, goods payment
/// and extra-charge payments together. One approval covers the whole request.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptApproval {
    /// A manager or owner PIN.
    pub manager_pin: String,
    /// Why more is paid out than the drawer holds.
    #[serde(default)]
    pub override_reason: Option<String>,
}
