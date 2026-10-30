use serde::Deserialize;
use utoipa::ToSchema;

/// The `customer{}` sub-object of `CreateQuotationRequest`. When `customerKey`
/// resolves to a real customer that record is authoritative for name, phone
/// and address; the typed values are only used for a walk-in with no key.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationCustomer {
    /// Registered customer key.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name (required when there is no `customerKey`).
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number.
    #[serde(default)]
    pub customer_phone: Option<String>,
    /// Walk-in customer address.
    #[serde(default)]
    pub customer_address: Option<String>,
}
