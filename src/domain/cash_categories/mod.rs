// Pure business types for drawer cash categories and the payee directory.
// No I/O, no framework types beyond serde/utoipa derives. Categories are data,
// not code: the shop grows its own list, and every cash movement points at one
// by `key`, so renaming a category rewrites history without touching rows.

mod cash_category;
mod cash_category_list_query;
mod category_key;
mod create_cash_category_request;
mod create_payee_request;
mod merge_cash_category_request;
mod normalize_name;
mod payee;
mod payee_key;
mod payee_list_query;
mod payee_type;
mod recurring_expense_key;
mod report_group;
mod review_status;
mod similar_cash_category_query;
#[cfg(test)]
mod tests;
mod update_cash_category_request;
mod update_payee_request;

pub use cash_category::CashCategory;
pub use cash_category_list_query::CashCategoryListQuery;
pub use category_key::category_key;
pub use create_cash_category_request::CreateCashCategoryRequest;
pub use create_payee_request::CreatePayeeRequest;
pub use merge_cash_category_request::MergeCashCategoryRequest;
pub use normalize_name::normalize_name;
pub use payee::Payee;
pub use payee_key::payee_key;
pub use payee_list_query::PayeeListQuery;
pub use payee_type::PayeeType;
pub use recurring_expense_key::recurring_expense_key;
pub use report_group::ReportGroup;
pub use review_status::ReviewStatus;
pub use similar_cash_category_query::SimilarCashCategoryQuery;
pub use update_cash_category_request::UpdateCashCategoryRequest;
pub use update_payee_request::UpdatePayeeRequest;
