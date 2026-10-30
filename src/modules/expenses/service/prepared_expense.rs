use crate::modules::expenses::model::{ExpenseDocument, ExpensePaymentDocument};

/// A validated expense, and the payment that settles it when it is paid in full on entry.
#[derive(Debug)]
pub(crate) struct PreparedExpense {
    pub(super) expense: ExpenseDocument,
    pub(super) payment: Option<ExpensePaymentDocument>,
}

impl PreparedExpense {
    /// The key of the payment row, when the expense is paid in full on entry.
    pub(crate) fn payment_key(&self) -> Option<&str> {
        self.payment.as_ref().map(|p| p.key.as_str())
    }
}
