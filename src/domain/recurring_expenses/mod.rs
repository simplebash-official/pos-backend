// Pure business types for regular bills (rent, electricity, subscriptions): a template says what is
// usually paid, to whom, how much, on which day of the month, and from which account. Reminders and
// "already paid" checks are worked out from the template and the costs actually recorded, never stored.
// No I/O, no framework types beyond serde/utoipa derives.

mod bill_status;
mod create_recurring_expense_request;
mod period_payment;
mod period_status;
mod period_status_query;
mod recurring_bill;
mod recurring_expense;
mod recurring_expense_list_query;
mod update_recurring_expense_request;

pub use bill_status::BillStatus;
pub use create_recurring_expense_request::CreateRecurringExpenseRequest;
pub use period_payment::PeriodPayment;
pub use period_status::PeriodStatus;
pub use period_status_query::PeriodStatusQuery;
pub use recurring_bill::RecurringBill;
pub use recurring_expense::RecurringExpense;
pub use recurring_expense_list_query::RecurringExpenseListQuery;
pub use update_recurring_expense_request::UpdateRecurringExpenseRequest;
