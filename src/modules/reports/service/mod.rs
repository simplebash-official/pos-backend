// Business rules and reporting orchestration for the reports module.

pub(crate) mod analytics;
pub(crate) mod commissions;
pub(crate) mod daily_sales;
pub(crate) mod dashboard;
pub(crate) mod dates;
pub(crate) mod employee_earnings;
pub(crate) mod inventory;
pub(crate) mod monthly_profit;
pub(crate) mod outstanding;
pub(crate) mod report_payload;

pub(crate) use analytics::{
    get_analytics_cashier_performance, get_analytics_discounts, get_analytics_payment_methods,
    get_analytics_receivables_aging, get_analytics_refunds, get_analytics_sales_by_category,
    get_analytics_sales_patterns, get_analytics_summary, get_analytics_timeseries,
    get_analytics_top_customers,
};
pub(crate) use commissions::get_employee_commissions_report;
pub(crate) use daily_sales::get_daily_sales_report;
pub(crate) use dashboard::get_dashboard_overview;
pub(crate) use employee_earnings::get_employee_earnings;
pub(crate) use inventory::{get_inventory_valuation, get_top_products};
pub(crate) use monthly_profit::get_monthly_profit_report;
pub(crate) use outstanding::get_outstanding_report;
pub(crate) use report_payload::build_analytics_report_data;
