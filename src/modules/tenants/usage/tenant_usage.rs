use super::day_usage::DayUsage;

use mongodb::bson::doc;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantUsage {
    pub tenant_id: String,
    pub shop_code: String,
    pub name: String,
    pub suspended: bool,
    /// Invoices that were not voided, ever / in the last 30 days / in the last 7 days.
    pub bills_total: u64,
    pub bills_30d: u64,
    pub bills_7d: u64,
    /// Gross sales of those invoices in cents (credit notes are not subtracted).
    pub sales_total_cents: i64,
    pub sales_30d_cents: i64,
    /// ISO 8601 time of the newest non-voided invoice.
    pub last_bill_at: Option<String>,
    pub products: u64,
    pub customers: u64,
    /// Active staff accounts.
    pub users: u64,
    /// Registered desktop devices that were not revoked.
    pub devices: u64,
    /// ISO 8601 time of the most recent push or pull by any device.
    pub last_sync_at: Option<String>,
    /// The last 30 days, days without bills left out.
    pub bills_by_day: Vec<DayUsage>,
}
