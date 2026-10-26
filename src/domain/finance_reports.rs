// Pure business types for the money reports: profit and loss, costs by category and month, cash
// flow per account and what is still to pay. No I/O, no framework types beyond serde/utoipa.
//
// Nothing here is stored: every figure is worked out from the sales, the expense and income
// ledgers, the cash movements and the supplier balances at the moment it is asked for.

use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::{accounts::AccountKind, cash_categories::ReportGroup};

/// One category's total inside a section of a report.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category_key: String,
    pub category_name: String,
    pub total_cents: i64,
}

/// What the shop spent in one group of costs (running costs, staff pay), with its categories.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseGroupTotal {
    pub group: ReportGroup,
    pub total_cents: i64,
    pub categories: Vec<CategoryTotal>,
}

/// Response of `GET /finance-reports/profit-loss`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossResponse {
    /// `YYYY-MM-DD`, inclusive.
    pub from: String,
    pub to: String,
    /// What customers were billed (after discounts), voided bills left out.
    pub sales_cents: i64,
    /// Money handed back to customers on returns.
    pub refunds_cents: i64,
    pub net_sales_cents: i64,
    /// What the items that were sold had cost.
    pub cost_of_goods_cents: i64,
    pub gross_profit_cents: i64,
    /// Money received that is not a sale (scrap, commission, supplier refunds, interest).
    pub other_income: Vec<CategoryTotal>,
    pub other_income_cents: i64,
    /// Running costs and staff pay. Supplier stock payments are left out: stock is already in the cost of goods.
    pub expenses: Vec<ExpenseGroupTotal>,
    pub expenses_cents: i64,
    /// Gross profit plus other income, less costs. Negative when the shop made a loss.
    pub net_profit_cents: i64,
}

/// One month's cost.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthTotal {
    /// `YYYY-MM`.
    pub month: String,
    pub total_cents: i64,
}

/// One category's cost over the period, with its months.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBreakdown {
    pub category_key: String,
    pub category_name: String,
    pub report_group: ReportGroup,
    pub total_cents: i64,
    /// Share of the period's total cost, whole percent rounded half up.
    pub share_percent: i64,
    pub months: Vec<MonthTotal>,
}

/// One band of the monthly bar chart.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    /// The category's name; empty for the "everything else" band, which the screen names.
    pub name: String,
    /// True for the band that adds up every category outside the biggest few.
    pub is_other: bool,
    /// Every month in the period, oldest first.
    pub months: Vec<MonthTotal>,
}

/// Response of `GET /finance-reports/expenses`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseBreakdownResponse {
    pub from: String,
    pub to: String,
    pub total_cents: i64,
    /// Biggest first.
    pub categories: Vec<CategoryBreakdown>,
    /// The biggest categories, then one "other" band for the rest: what the monthly chart stacks.
    pub chart: Vec<ChartSeries>,
    /// Every month in the period, including months with no cost, oldest first.
    pub months: Vec<MonthTotal>,
}

/// What moved through an account for one reason.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowLine {
    /// A fixed word the screen turns into plain language: `sales`, `refunds`, `other_income`,
    /// `cash_added`, `paid_out`, `owner_withdrawals`, `cash_to_bank_or_safe`, `bank_deposits`,
    /// `bank_withdrawals`, `expense_payments`.
    pub code: String,
    pub in_cents: i64,
    pub out_cents: i64,
}

/// Money in and out of one account over the period.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountCashFlow {
    pub kind: AccountKind,
    pub in_cents: i64,
    pub out_cents: i64,
    pub net_cents: i64,
    pub lines: Vec<CashFlowLine>,
}

/// Response of `GET /finance-reports/cash-flow`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowResponse {
    pub from: String,
    pub to: String,
    pub accounts: Vec<AccountCashFlow>,
}

/// A bill that is not paid in full.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PayableBill {
    pub key: String,
    pub number: String,
    pub category_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    pub balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub is_overdue: bool,
}

/// A supplier the shop owes money to.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PayableSupplier {
    pub supplier_key: String,
    pub name: String,
    pub balance_cents: i64,
}

/// Response of `GET /finance-reports/payables`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PayablesResponse {
    /// `YYYY-MM-DD` (UTC) the figures are as of.
    pub as_of: String,
    /// Bills not paid in full, soonest due first; those without a due date last.
    pub bills: Vec<PayableBill>,
    pub bills_cents: i64,
    pub overdue_cents: i64,
    /// Suppliers with a balance owed, biggest first. Suppliers without an opening balance are not listed
    /// (their balance is not known, see the Balances screen).
    pub suppliers: Vec<PayableSupplier>,
    pub suppliers_cents: i64,
    pub total_cents: i64,
}
