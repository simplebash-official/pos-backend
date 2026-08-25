// Business rules and reporting orchestration for the reports module.

pub(crate) mod commissions;
pub(crate) mod daily_sales;
pub(crate) mod dashboard;
pub(crate) mod dates;
pub(crate) mod employee_earnings;
pub(crate) mod inventory;
pub(crate) mod monthly_profit;
pub(crate) mod outstanding;

pub(crate) use commissions::get_employee_commissions_report;
pub(crate) use daily_sales::get_daily_sales_report;
pub(crate) use dashboard::get_dashboard_overview;
pub(crate) use employee_earnings::get_employee_earnings;
pub(crate) use inventory::{get_inventory_valuation, get_top_products};
pub(crate) use monthly_profit::get_monthly_profit_report;
pub(crate) use outstanding::get_outstanding_report;
