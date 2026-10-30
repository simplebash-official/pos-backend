//! Business rules for the money reports: which day range is allowed and how each figure is put
//! together from the ledgers. Nothing is stored; every call reads the current records.

mod finance_reports_service;

pub use finance_reports_service::cash_flow;
pub use finance_reports_service::expense_breakdown;
pub use finance_reports_service::payables;
pub use finance_reports_service::profit_loss;
