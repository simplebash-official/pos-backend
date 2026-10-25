// Pure business types for the expense ledger: every business cost, whichever account it
// moved through. No I/O, no framework types beyond serde/utoipa derives.
//
// Two sources feed one list. Costs paid from the drawer are entered once, as a cash movement
// (rent, bills, staff pay, supplier pay, small expenses), and appear here by being read from
// the movements, never copied, so nothing can be counted twice. Costs paid any other way, or
// not paid yet, are entered here as an `Expense` with one or more `ExpensePayment`s.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::{cash_categories::ReportGroup, sessions::CashMovementType};

/// Where the money for an expense or a payment comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaidFrom {
    /// The cash drawer. Never entered here: a drawer payment is a cash movement.
    Drawer,
    Bank,
    Card,
    Online,
    /// Entered as a bill still to be paid.
    Unpaid,
}

impl PaidFrom {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaidFrom::Drawer => "drawer",
            PaidFrom::Bank => "bank",
            PaidFrom::Card => "card",
            PaidFrom::Online => "online",
            PaidFrom::Unpaid => "unpaid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(PaidFrom::Drawer),
            "bank" => Some(PaidFrom::Bank),
            "card" => Some(PaidFrom::Card),
            "online" => Some(PaidFrom::Online),
            "unpaid" => Some(PaidFrom::Unpaid),
            _ => None,
        }
    }

    /// Whether this is a real account money can leave from (everything but `Unpaid`).
    pub fn is_account(&self) -> bool {
        !matches!(self, PaidFrom::Unpaid)
    }
}

/// How much of an expense has been paid. Worked out from its payments, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseStatus {
    Unpaid,
    PartPaid,
    Paid,
}

impl ExpenseStatus {
    pub fn of(amount_cents: i64, paid_cents: i64) -> Self {
        if paid_cents <= 0 {
            ExpenseStatus::Unpaid
        } else if paid_cents < amount_cents {
            ExpenseStatus::PartPaid
        } else {
            ExpenseStatus::Paid
        }
    }
}

/// Whether a row of the list came from the ledger or from the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseSource {
    Ledger,
    Drawer,
}

/// One payment against an expense.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpensePayment {
    pub id: String,
    pub key: String,
    pub expense_key: String,
    pub amount_cents: i64,
    pub paid_from: PaidFrom,
    /// The day the money left (`YYYY-MM-DD`).
    pub paid_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}

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

/// A ledger expense with its payments.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseDetail {
    pub expense: ExpenseRow,
    pub payments: Vec<ExpensePayment>,
}

/// Body for `POST /expenses`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateExpenseRequest {
    /// `YYYY-MM-DD`.
    pub expense_date: String,
    pub category_key: String,
    #[serde(default)]
    pub payee_key: Option<String>,
    #[serde(default)]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    /// `bank`, `card`, `online` (paid now, in full) or `unpaid` (a bill to pay later).
    pub paid_from: PaidFrom,
    /// `YYYY-MM-DD`; only for an unpaid bill.
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// For a supplier payment paid on entry: the supplier receipts (invoices) it pays, and how
    /// much of it each.
    #[serde(default)]
    pub settlements: Vec<crate::domain::purchase_receipts::ReceiptSettlementRequest>,
}

/// Body for `POST /expenses/{key}/payments`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateExpensePaymentRequest {
    pub amount_cents: i64,
    /// `bank`, `card` or `online`.
    pub paid_from: PaidFrom,
    /// `YYYY-MM-DD`.
    pub paid_on: String,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Query for `GET /expenses`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ExpenseListQuery {
    pub search: Option<String>,
    pub category_key: Option<String>,
    pub payee_key: Option<String>,
    /// `unpaid` (anything with a balance), `overdue` or `paid`.
    pub status: Option<String>,
    /// `ledger` or `drawer`.
    pub source: Option<String>,
    /// `YYYY-MM-DD`, inclusive.
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// A page of the combined list.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseListResponse {
    pub expenses: Vec<ExpenseRow>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// Headline figures for the Expenses page.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseStats {
    /// Everything spent this month (UTC), whatever it was paid from.
    pub this_month_cents: i64,
    /// Of that, paid from the drawer.
    pub this_month_drawer_cents: i64,
    /// Still owed on bills, all months.
    pub unpaid_cents: i64,
    pub unpaid_count: u64,
    /// Of the unpaid, past their due date.
    pub overdue_cents: i64,
    pub overdue_count: u64,
}
