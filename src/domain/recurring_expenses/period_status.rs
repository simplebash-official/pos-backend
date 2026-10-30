use super::period_payment::PeriodPayment;

use serde::Serialize;
use utoipa::ToSchema;

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
