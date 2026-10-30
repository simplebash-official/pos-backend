/// Counts read from the shop's own data (no amounts, no record content).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Retained {
    pub bills: u64,
    pub products: u64,
    pub customers: u64,
    pub staff: u64,
    /// RFC 3339 time of the newest non-voided invoice.
    pub last_bill_at: Option<String>,
}
