//! Pure business types for regular bills (rent, electricity, subscriptions): a template says what is
//! usually paid, to whom, how much, on which day of the month, and from which account. Reminders and
//! "already paid" checks are worked out from the template and the costs actually recorded, never stored.
//! No I/O, no framework types beyond serde/utoipa derives.

mod domain_recurring_expenses;

pub use domain_recurring_expenses::BillStatus;
pub use domain_recurring_expenses::CreateRecurringExpenseRequest;
pub use domain_recurring_expenses::PeriodPayment;
pub use domain_recurring_expenses::PeriodStatus;
pub use domain_recurring_expenses::PeriodStatusQuery;
pub use domain_recurring_expenses::RecurringBill;
pub use domain_recurring_expenses::RecurringExpense;
pub use domain_recurring_expenses::RecurringExpenseListQuery;
pub use domain_recurring_expenses::UpdateRecurringExpenseRequest;
