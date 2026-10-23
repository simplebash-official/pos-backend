// Pure business types for regular bills (rent, electricity, subscriptions): a template says what is
// usually paid, to whom, how much, on which day of the month, and from which account. Reminders and
// "already paid" checks are worked out from the template and the costs actually recorded, never stored.
// No I/O, no framework types beyond serde/utoipa derives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::expenses::PaidFrom;

/// A regular bill the shop pays every month.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecurringExpense {
    pub id: String,
    pub key: String,
    pub category_key: String,
    /// Live name of the category, so a rename shows here.
    pub category_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    /// What is usually paid each month.
    pub amount_cents: i64,
    /// Day of the month it is due, 1 to 31; a month with fewer days uses its last day.
    pub due_day: i64,
    /// How many days before the due day it starts showing as a reminder.
    pub remind_days_before: i64,
    /// Where it is usually paid from: `drawer`, `bank`, `card` or `online`.
    pub paid_from: PaidFrom,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Stopped templates stay on the record but no longer remind.
    pub active: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Where a bill stands for one month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BillStatus {
    /// Not yet inside its reminder window.
    Upcoming,
    /// Inside the reminder window, or due today, and not paid in full.
    Due,
    /// Past its due day and not paid in full.
    Overdue,
    Paid,
}

/// One cost already recorded against a bill's month, from the drawer or the ledger.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PeriodPayment {
    /// `EXP-000001` or the voucher number `CSH-000001`.
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub amount_cents: i64,
    pub paid_from: PaidFrom,
}

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

/// What is already recorded for one category and payee in one month, for the "already paid" warning.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStatus {
    /// `YYYY-MM`.
    pub period: String,
    pub paid_cents: i64,
    /// What the regular bill for this category and payee says is usual, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_cents: Option<i64>,
    /// Paid in full when there is a regular bill, or anything recorded when there is not.
    pub already_paid: bool,
    pub payments: Vec<PeriodPayment>,
}

/// Body for `POST /recurring-expenses`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecurringExpenseRequest {
    pub category_key: String,
    #[serde(default)]
    pub payee_key: Option<String>,
    pub amount_cents: i64,
    pub due_day: i64,
    pub remind_days_before: i64,
    pub paid_from: PaidFrom,
    #[serde(default)]
    pub note: Option<String>,
}

/// Body for `PATCH /recurring-expenses/{key}`. Absent fields are left unchanged; the category and
/// payee are what identify a regular bill, so they cannot be changed (stop it and add a new one).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecurringExpenseRequest {
    #[serde(default)]
    pub amount_cents: Option<i64>,
    #[serde(default)]
    pub due_day: Option<i64>,
    #[serde(default)]
    pub remind_days_before: Option<i64>,
    #[serde(default)]
    pub paid_from: Option<PaidFrom>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Query for `GET /recurring-expenses`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RecurringExpenseListQuery {
    /// Stopped templates are left out unless this is true.
    pub include_inactive: Option<bool>,
}

/// Query for `GET /recurring-expenses/period-status`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PeriodStatusQuery {
    pub category_key: String,
    pub payee_key: Option<String>,
    /// `YYYY-MM`.
    pub period: String,
}
