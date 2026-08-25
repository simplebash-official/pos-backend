//! Fixed permission strings ("<module>:<action>") checked against a
//! caller's JWT-embedded permission list via
//! `core::middleware::auth::CurrentUser::require_permission`. Adding a new
//! permission for a future module is two edits: add the constant here, then
//! slot it into whichever role(s) should have it by default in
//! `core::constants::roles`.

/// Full account management (all roles except Admin — see
/// `core::constants::roles`'s `manageable_roles`-equivalent restriction in
/// `modules::users::service`) — granted to Admin.
pub const USERS_MANAGE: &str = "users:manage";
/// Staff-account-only management — granted to Manager, who may create/edit/
/// delete Staff accounts but not Manager or Admin accounts.
pub const USERS_MANAGE_STAFF: &str = "users:manage:staff";
pub const SESSIONS_VIEW: &str = "sessions:view";

/// Read access to the catalog, stock levels, the stock-movement audit trail
/// and the category tree — every `GET` under `/api/inventory` except the
/// module-status stub. Granted to all three roles.
pub const INVENTORY_READ: &str = "inventory:read";
/// Product create/update/delete and stock adjustment. Granted to Admin and
/// Manager only, so a Staff token can read the catalog but never mutate it.
pub const INVENTORY_WRITE: &str = "inventory:write";
/// Reserved for a future finer-grained split of inventory administration.
/// Deliberately unenforced: category/subcategory management is gated by
/// `AdminUser` directly (see `modules::inventory::routes`), matching the
/// `suppliers`/`supplier_products`/`purchases` note below.
pub const INVENTORY_ADMIN: &str = "inventory:admin";
/// Customer create/update/delete. Reads require only a valid token, since
/// all three roles hold this permission anyway.
pub const CUSTOMERS_WRITE: &str = "customers:write";

// Placeholders for modules still being built out — no route currently
// enforces these; wire a real `require_permission` check to one as its
// module gains real write endpoints, following the pattern `modules::users`
// and `modules::inventory` already establish.
// (`suppliers`/`supplier_products`/`purchases` are deliberately *not* here —
// those modules are gated by `AdminUser` directly, not a permission, since
// only Admin may touch them at all.)
pub const BILLING_WRITE: &str = "billing:write";
pub const REPAIRS_WRITE: &str = "repairs:write";
pub const PRINT_JOBS_WRITE: &str = "print_jobs:write";
pub const REPORTS_VIEW: &str = "reports:view";

/// Read access to the employee HR/commission directory — every `GET` under
/// `/api/employees`. Granted to all three roles, since Staff genuinely needs
/// to browse/pick an employee when assigning a repair or print job.
pub const EMPLOYEES_READ: &str = "employees:read";
/// Employee profile create/update/delete, including the default commission
/// split. Granted to Admin and Manager only — mirrors `INVENTORY_READ`/
/// `INVENTORY_WRITE`'s read/write split, not `USERS_MANAGE`/
/// `USERS_MANAGE_STAFF`'s target-hierarchy split, since employees carry no
/// role hierarchy among themselves. Creating a *login* for an employee is a
/// separate action gated by `USERS_MANAGE`/`USERS_MANAGE_STAFF` instead (see
/// `modules::users::service`), not this permission.
pub const EMPLOYEES_WRITE: &str = "employees:write";
