// HTTP layer for reports and financial intelligence: executive dashboard KPIs, daily sales,
// monthly profit/loss, credit receivables aging, employee commission payouts, inventory valuation,
// and top selling products.

mod get_analytics_cashier_performance;
mod get_analytics_discounts;
mod get_analytics_document;
mod get_analytics_payment_methods;
mod get_analytics_receivables_aging;
mod get_analytics_refunds;
mod get_analytics_sales_by_category;
mod get_analytics_sales_patterns;
mod get_analytics_summary;
mod get_analytics_timeseries;
mod get_analytics_top_customers;
mod get_daily_sales;
mod get_dashboard_overview;
mod get_dashboard_pulse;
mod get_employee_commissions;
mod get_employee_earnings_items;
mod get_engine_feed;
mod get_inventory_valuation;
mod get_monthly_profit;
mod get_outstanding_receivables;
mod get_reminders;
mod get_top_products;
mod invalidate_engine_cache;
mod router;
mod status;

pub use router::router;
