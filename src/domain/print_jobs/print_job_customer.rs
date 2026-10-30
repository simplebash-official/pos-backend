use serde::Deserialize;
use utoipa::ToSchema;

/// The `customer{}` sub-object of `CreatePrintJobRequest`/
/// `UpdatePrintJobRequest` — same shape and override rule as
/// `repairs::RepairCustomer`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobCustomer {
    /// Registered customer key.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name.
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number.
    #[serde(default)]
    pub customer_phone: Option<String>,
}
