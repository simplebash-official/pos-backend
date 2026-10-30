use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How much of an expense has been paid. Worked out from its payments, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseStatus {
    Unpaid,
    PartPaid,
    Paid,
}

impl ExpenseStatus {
    #[must_use]
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
