// Pure business types for the money reports: profit and loss, costs by category and month, cash
// flow per account and what is still to pay. No I/O, no framework types beyond serde/utoipa.
//
// Nothing here is stored: every figure is worked out from the sales, the expense and income
// ledgers, the cash movements and the supplier balances at the moment it is asked for.

mod account_cash_flow;
mod cash_flow_line;
mod cash_flow_response;
mod category_breakdown;
mod category_total;
mod chart_series;
mod expense_breakdown_response;
mod expense_group_total;
mod month_total;
mod payable_bill;
mod payable_supplier;
mod payables_response;
mod profit_loss_response;

pub use account_cash_flow::AccountCashFlow;
pub use cash_flow_line::CashFlowLine;
pub use cash_flow_response::CashFlowResponse;
pub use category_breakdown::CategoryBreakdown;
pub use category_total::CategoryTotal;
pub use chart_series::ChartSeries;
pub use expense_breakdown_response::ExpenseBreakdownResponse;
pub use expense_group_total::ExpenseGroupTotal;
pub use month_total::MonthTotal;
pub use payable_bill::PayableBill;
pub use payable_supplier::PayableSupplier;
pub use payables_response::PayablesResponse;
pub use profit_loss_response::ProfitLossResponse;
