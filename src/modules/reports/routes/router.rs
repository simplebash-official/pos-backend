use super::get_analytics_cashier_performance;
use super::get_analytics_discounts;
use super::get_analytics_document;
use super::get_analytics_payment_methods;
use super::get_analytics_receivables_aging;
use super::get_analytics_refunds;
use super::get_analytics_sales_by_category;
use super::get_analytics_sales_patterns;
use super::get_analytics_summary;
use super::get_analytics_timeseries;
use super::get_analytics_top_customers;
use super::get_daily_sales;
use super::get_dashboard_overview;
use super::get_dashboard_pulse;
use super::get_employee_commissions;
use super::get_employee_earnings_items;
use super::get_engine_feed;
use super::get_inventory_valuation;
use super::get_monthly_profit;
use super::get_outstanding_receivables;
use super::get_reminders;
use super::get_top_products;
use super::invalidate_engine_cache;
use super::status;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Module status
        .routes(routes!(status::status))
        // High-speed Analytics & Chart Engine
        .routes(routes!(get_engine_feed::get_engine_feed))
        .routes(routes!(invalidate_engine_cache::invalidate_engine_cache))
        // Executive overview & KPIs
        .routes(routes!(get_dashboard_overview::get_dashboard_overview))
        // Sales & Profit intelligence
        .routes(routes!(get_daily_sales::get_daily_sales))
        .routes(routes!(get_monthly_profit::get_monthly_profit))
        // Receivables & Outstanding credit
        .routes(routes!(
            get_outstanding_receivables::get_outstanding_receivables
        ))
        .routes(routes!(get_reminders::get_reminders))
        .routes(routes!(get_dashboard_pulse::get_dashboard_pulse))
        // Employee commissions & performance
        .routes(routes!(get_employee_commissions::get_employee_commissions))
        .routes(routes!(
            get_employee_earnings_items::get_employee_earnings_items
        ))
        // Inventory valuation & Product performance
        .routes(routes!(get_inventory_valuation::get_inventory_valuation))
        .routes(routes!(get_top_products::get_top_products))
        // Analytics & Reports
        .routes(routes!(get_analytics_summary::get_analytics_summary))
        .routes(routes!(get_analytics_timeseries::get_analytics_timeseries))
        .routes(routes!(
            get_analytics_payment_methods::get_analytics_payment_methods
        ))
        .routes(routes!(
            get_analytics_top_customers::get_analytics_top_customers
        ))
        .routes(routes!(
            get_analytics_sales_by_category::get_analytics_sales_by_category
        ))
        .routes(routes!(
            get_analytics_cashier_performance::get_analytics_cashier_performance
        ))
        .routes(routes!(
            get_analytics_sales_patterns::get_analytics_sales_patterns
        ))
        .routes(routes!(
            get_analytics_receivables_aging::get_analytics_receivables_aging
        ))
        .routes(routes!(get_analytics_discounts::get_analytics_discounts))
        .routes(routes!(get_analytics_refunds::get_analytics_refunds))
        .routes(routes!(get_analytics_document::get_analytics_document))
}
