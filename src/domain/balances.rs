// Pure business types for what the shop owes suppliers and what staff owe on advances.
// No I/O, no framework types beyond serde/utoipa derives.
//
// Nothing here is stored as a balance. A supplier balance is the amount owed on a chosen day plus
// purchases, less payments, since then. A staff advance is what was handed out (drawer or ledger),
// less what was recovered from pay.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whose balance an opening belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BalanceSubject {
    Supplier,
    Staff,
}

impl BalanceSubject {
    pub fn as_str(&self) -> &'static str {
        match self {
            BalanceSubject::Supplier => "supplier",
            BalanceSubject::Staff => "staff",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supplier" => Some(BalanceSubject::Supplier),
            "staff" => Some(BalanceSubject::Staff),
            _ => None,
        }
    }
}

/// What the shop owes one supplier. A supplier has no balance until an opening balance is set,
/// because stock may have been paid for before payments were tracked here.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBalance {
    pub supplier_key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_balance_cents: Option<i64>,
    /// `YYYY-MM-DD`: purchases and payments from this day on count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_date: Option<String>,
    /// Purchases (quantity x unit cost) since the opening day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchased_cents: Option<i64>,
    /// Paid from the drawer, the bank, a card or online since the opening day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_cents: Option<i64>,
    /// Taken off by supplier credits (short or damaged goods, price differences) since the opening day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credited_cents: Option<i64>,
    /// What is owed; negative when the shop has paid ahead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBalancesResponse {
    pub suppliers: Vec<SupplierBalance>,
}

/// What one staff member still owes on advances and staff loans.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffAdvance {
    pub employee_key: String,
    pub name: String,
    /// Owed before the records began, when the owner entered it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_balance_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_date: Option<String>,
    /// Advances and staff loans handed out (counted from the opening day when there is one).
    pub given_cents: i64,
    pub recovered_cents: i64,
    pub outstanding_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_given_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffAdvancesResponse {
    pub staff: Vec<StaffAdvance>,
}

/// One advance handed out.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceGiven {
    /// The voucher number (`CSH-000001`) or the expense number (`EXP-000001`).
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub amount_cents: i64,
    pub category_name: String,
}

/// Part of an advance taken back from pay.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceRecovery {
    pub key: String,
    pub employee_key: String,
    pub amount_cents: i64,
    /// `YYYY-MM-DD`.
    pub recovered_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffAdvanceDetail {
    pub summary: StaffAdvance,
    pub given: Vec<AdvanceGiven>,
    pub recoveries: Vec<AdvanceRecovery>,
}

/// Body for `PUT /balances/{kind}/{key}/opening`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetBalanceOpeningRequest {
    /// What was owed on `asOfDate` (for a supplier, what the shop owed; for staff, what they owed the shop).
    pub opening_balance_cents: i64,
    /// `YYYY-MM-DD`, today or earlier.
    pub as_of_date: String,
}

/// Body for `POST /balances/staff/{employeeKey}/recoveries`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateAdvanceRecoveryRequest {
    pub amount_cents: i64,
    /// `YYYY-MM-DD`.
    pub recovered_on: String,
    #[serde(default)]
    pub note: Option<String>,
}
