// Pure business types for the expense ledger: every business cost, whichever account it
// moved through. No I/O, no framework types beyond serde/utoipa derives.
//
// Two sources feed one list. Costs paid from the drawer are entered once, as a cash movement
// (rent, bills, staff pay, supplier pay, small expenses), and appear here by being read from
// the movements, never copied, so nothing can be counted twice. Costs paid any other way, or
// not paid yet, are entered here as an `Expense` with one or more `ExpensePayment`s.

mod create_expense_payment_request;
mod create_expense_request;
mod expense_detail;
mod expense_list_query;
mod expense_list_response;
mod expense_payment;
mod expense_row;
mod expense_source;
mod expense_stats;
mod expense_status;
mod paid_from;

pub use create_expense_payment_request::CreateExpensePaymentRequest;
pub use create_expense_request::CreateExpenseRequest;
pub use expense_detail::ExpenseDetail;
pub use expense_list_query::ExpenseListQuery;
pub use expense_list_response::ExpenseListResponse;
pub use expense_payment::ExpensePayment;
pub use expense_row::ExpenseRow;
pub use expense_source::ExpenseSource;
pub use expense_stats::ExpenseStats;
pub use expense_status::ExpenseStatus;
pub use paid_from::PaidFrom;
