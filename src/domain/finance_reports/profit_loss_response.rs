use super::category_total::CategoryTotal;
use super::expense_group_total::ExpenseGroupTotal;

use serde::Serialize;
use utoipa::ToSchema;

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
