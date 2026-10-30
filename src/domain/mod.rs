// Pure business types shared across modules — no I/O, no framework types.
// Populated as domain modules (billing, repairs, inventory, ...) grow.

pub mod accounts;
pub mod attachments;
pub mod auth;
pub mod backup;
pub mod balances;
pub mod billing;
pub mod cash_categories;
pub mod cash_settings;
pub mod customers;
pub mod employees;
pub mod expenses;
pub mod finance_reports;
mod health_response;
pub mod imports;
pub mod incomes;
pub mod inventory;
pub mod labels;
mod module_status_response;
pub mod print_jobs;
pub mod purchase_receipts;
pub mod purchases;
pub mod quotations;
pub mod recurring_expenses;
pub mod repairs;
pub mod reports;
pub mod sequences;
pub mod sessions;
pub mod settings;
pub mod supplier_credits;
pub mod supplier_products;
pub mod suppliers;
pub mod sync;
pub mod sync_local;
pub mod sync_v2;
pub mod system;
pub mod users;

pub use health_response::HealthResponse;
pub use module_status_response::ModuleStatusResponse;
