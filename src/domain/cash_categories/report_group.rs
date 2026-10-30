use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How a category is counted in profit and loss. Inherited from the movement
/// type when a category is created, so a cashier is never asked about accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportGroup {
    /// Counts as an expense in profit and loss.
    OperatingExpense,
    /// Supplier stock payments: cost of goods, not an expense.
    Purchases,
    /// Salary, advances, EPF and ETF: staff cost.
    Payroll,
    /// Owner capital and drawings: excluded from profit and loss.
    OwnerEquity,
    /// Cash drops, floats and no-sale opens: excluded from profit and loss.
    Transfer,
    /// Loans received and principal repaid: only the interest part is an expense.
    Liabilities,
    /// Money received that is not a sale (scrap, commission, supplier refunds, interest):
    /// counts as income in profit and loss.
    OtherIncome,
}

impl ReportGroup {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportGroup::OperatingExpense => "operating_expense",
            ReportGroup::Purchases => "purchases",
            ReportGroup::Payroll => "payroll",
            ReportGroup::OwnerEquity => "owner_equity",
            ReportGroup::Transfer => "transfer",
            ReportGroup::Liabilities => "liabilities",
            ReportGroup::OtherIncome => "other_income",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "operating_expense" => Some(ReportGroup::OperatingExpense),
            "purchases" => Some(ReportGroup::Purchases),
            "payroll" => Some(ReportGroup::Payroll),
            "owner_equity" => Some(ReportGroup::OwnerEquity),
            "transfer" => Some(ReportGroup::Transfer),
            "liabilities" => Some(ReportGroup::Liabilities),
            "other_income" => Some(ReportGroup::OtherIncome),
            _ => None,
        }
    }
}
