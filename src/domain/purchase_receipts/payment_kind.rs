use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What kind of payment record a settlement points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentKind {
    /// A drawer `supplier_payment` cash movement.
    CashMovement,
    /// A payment on a ledger expense (bank, card or online).
    ExpensePayment,
}

impl PaymentKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentKind::CashMovement => "cash_movement",
            PaymentKind::ExpensePayment => "expense_payment",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cash_movement" => Some(PaymentKind::CashMovement),
            "expense_payment" => Some(PaymentKind::ExpensePayment),
            _ => None,
        }
    }
}
