use serde::Deserialize;
use utoipa::ToSchema;

/// The `customer{}` sub-object of `CreateSaleRequest`. `customerName`/
/// `Phone`/`Address` are only used as the walk-in snapshot when
/// `customerKey` is absent; when a real customer is linked,
/// `service::sale::complete_sale` snapshots from the resolved `Customer`
/// record instead (server-authoritative, not client-supplied).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerRef {
    /// Key of an existing registered customer.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name if not registered.
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number if not registered.
    #[serde(default)]
    pub customer_phone: Option<String>,
    /// Walk-in customer address if not registered.
    #[serde(default)]
    pub customer_address: Option<String>,
}
