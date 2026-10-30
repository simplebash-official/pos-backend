use super::income_source::IncomeSource;
use super::received_into::ReceivedInto;
use crate::domain::cash_categories::ReportGroup;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Money received, as one row of the list: either a ledger income or a drawer add-cash.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncomeRow {
    /// The income key, or the cash movement key for a drawer row.
    pub key: String,
    pub source: IncomeSource,
    /// `INC-000001` or the voucher number `CSH-000001`.
    pub number: String,
    /// `YYYY-MM-DD`; for a drawer row the day it was recorded (UTC).
    pub date: String,
    pub category_key: String,
    pub category_name: String,
    pub report_group: ReportGroup,
    /// Who it came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    pub received_into: ReceivedInto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}
