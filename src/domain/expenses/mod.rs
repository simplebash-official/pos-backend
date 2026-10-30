//! Pure business types for the expense ledger: every business cost, whichever account it
//! moved through. No I/O, no framework types beyond serde/utoipa derives.
//!
//! Two sources feed one list. Costs paid from the drawer are entered once, as a cash movement
//! (rent, bills, staff pay, supplier pay, small expenses), and appear here by being read from
//! the movements, never copied, so nothing can be counted twice. Costs paid any other way, or
//! not paid yet, are entered here as an `Expense` with one or more `ExpensePayment`s.

mod domain_expenses;

pub use domain_expenses::CreateExpensePaymentRequest;
pub use domain_expenses::CreateExpenseRequest;
pub use domain_expenses::ExpenseDetail;
pub use domain_expenses::ExpenseListQuery;
pub use domain_expenses::ExpenseListResponse;
pub use domain_expenses::ExpensePayment;
pub use domain_expenses::ExpenseRow;
pub use domain_expenses::ExpenseSource;
pub use domain_expenses::ExpenseStats;
pub use domain_expenses::ExpenseStatus;
pub use domain_expenses::PaidFrom;
