//! Pure business types for the money reports: profit and loss, costs by category and month, cash
//! flow per account and what is still to pay. No I/O, no framework types beyond serde/utoipa.
//!
//! Nothing here is stored: every figure is worked out from the sales, the expense and income
//! ledgers, the cash movements and the supplier balances at the moment it is asked for.

mod domain_finance_reports;

pub use domain_finance_reports::AccountCashFlow;
pub use domain_finance_reports::CashFlowLine;
pub use domain_finance_reports::CashFlowResponse;
pub use domain_finance_reports::CategoryBreakdown;
pub use domain_finance_reports::CategoryTotal;
pub use domain_finance_reports::ChartSeries;
pub use domain_finance_reports::ExpenseBreakdownResponse;
pub use domain_finance_reports::ExpenseGroupTotal;
pub use domain_finance_reports::MonthTotal;
pub use domain_finance_reports::PayableBill;
pub use domain_finance_reports::PayableSupplier;
pub use domain_finance_reports::PayablesResponse;
pub use domain_finance_reports::ProfitLossResponse;
