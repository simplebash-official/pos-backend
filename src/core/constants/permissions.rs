//! Fixed permission strings ("<module>:<action>") checked against a
//! caller's JWT-embedded permission list via
//! `core::middleware::auth::CurrentUser::require_permission`. Adding a new
//! permission for a future module is two edits: add the constant here, then
//! slot it into whichever role(s) should have it by default in
//! `core::constants::roles`.

pub const USERS_MANAGE: &str = "users:manage";
pub const SESSIONS_VIEW: &str = "sessions:view";

// Placeholders for modules still being built out — no route currently
// enforces these; wire a real `require_permission` check to one as its
// module gains real write endpoints, following the pattern `modules::users`
// already establishes.
pub const INVENTORY_READ: &str = "inventory:read";
pub const INVENTORY_WRITE: &str = "inventory:write";
pub const INVENTORY_ADMIN: &str = "inventory:admin";
pub const SUPPLIERS_WRITE: &str = "suppliers:write";
pub const PURCHASES_WRITE: &str = "purchases:write";
pub const BILLING_WRITE: &str = "billing:write";
pub const REPAIRS_WRITE: &str = "repairs:write";
pub const PRINT_JOBS_WRITE: &str = "print_jobs:write";
pub const REPORTS_VIEW: &str = "reports:view";
pub const CUSTOMERS_WRITE: &str = "customers:write";
