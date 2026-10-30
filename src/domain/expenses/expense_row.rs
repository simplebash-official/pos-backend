use super::expense_source::ExpenseSource;
use super::expense_status::ExpenseStatus;
use super::paid_from::PaidFrom;
use crate::domain::cash_categories::ReportGroup;
use crate::domain::sessions::CashMovementType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A cost, as one row of the list: either a ledger expense or a drawer payment.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseRow {
    /// The expense key, or the cash movement key for a drawer row.
    pub key: String,
    pub source: ExpenseSource,
    /// `EXP-000001` or the voucher number `CSH-000001`.
    pub number: String,
    /// `YYYY-MM-DD`; for a drawer row the day it was recorded (UTC).
    pub date: String,
    pub category_key: String,
    pub category_name: String,
    pub report_group: ReportGroup,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    pub paid_cents: i64,
    pub balance_cents: i64,
    pub status: ExpenseStatus,
    /// Where it was paid from; `unpaid` while nothing has been paid, and the last payment's
    /// account once part-paid or paid.
    pub paid_from: PaidFrom,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// First day of the period a drawer payment covers (rent, bills, pay), when it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<String>,
    /// Not paid in full and past its due date.
    pub is_overdue: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// For a drawer row, the kind of cash movement it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movement_type: Option<CashMovementType>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}
