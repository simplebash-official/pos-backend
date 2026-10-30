//! HTTP layer for reports and financial intelligence: executive dashboard KPIs, daily sales,
//! monthly profit/loss, credit receivables aging, employee commission payouts, inventory valuation,
//! and top selling products.

mod reports_routes;

pub use reports_routes::router;
