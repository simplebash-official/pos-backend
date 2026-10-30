use super::bill_status::BillStatus;
use super::period_payment::PeriodPayment;
use super::recurring_expense::RecurringExpense;

use serde::Serialize;
use utoipa::ToSchema;

/// A template with how it stands for the month that needs attention: the earliest unpaid of last
/// month and this month, else this month.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecurringBill {
    pub template: RecurringExpense,
    /// `YYYY-MM`.
    pub period: String,
    /// `YYYY-MM-DD`.
    pub due_date: String,
    /// Negative once overdue.
    pub days_until_due: i64,
    pub paid_cents: i64,
    pub balance_cents: i64,
    pub status: BillStatus,
    pub payments: Vec<PeriodPayment>,
}
