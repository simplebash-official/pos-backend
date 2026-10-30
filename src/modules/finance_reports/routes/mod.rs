//! HTTP layer for the money reports. Read-only; every route needs `reports:view` (Admin and Manager).

mod finance_reports_routes;

pub use finance_reports_routes::router;
