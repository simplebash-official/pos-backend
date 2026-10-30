//! Pure business types for drawer cash categories and the payee directory.
//! No I/O, no framework types beyond serde/utoipa derives. Categories are data,
//! not code: the shop grows its own list, and every cash movement points at one
//! by `key`, so renaming a category rewrites history without touching rows.

mod domain_cash_categories;
#[cfg(test)]
mod tests;

pub use domain_cash_categories::CashCategory;
pub use domain_cash_categories::CashCategoryListQuery;
pub use domain_cash_categories::CreateCashCategoryRequest;
pub use domain_cash_categories::CreatePayeeRequest;
pub use domain_cash_categories::MergeCashCategoryRequest;
pub use domain_cash_categories::Payee;
pub use domain_cash_categories::PayeeListQuery;
pub use domain_cash_categories::PayeeType;
pub use domain_cash_categories::ReportGroup;
pub use domain_cash_categories::ReviewStatus;
pub use domain_cash_categories::SimilarCashCategoryQuery;
pub use domain_cash_categories::UpdateCashCategoryRequest;
pub use domain_cash_categories::UpdatePayeeRequest;
pub use domain_cash_categories::category_key;
pub use domain_cash_categories::normalize_name;
pub use domain_cash_categories::payee_key;
pub use domain_cash_categories::recurring_expense_key;
