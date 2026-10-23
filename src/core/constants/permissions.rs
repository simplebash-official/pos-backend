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
/// Lists the shop's login sessions (`/api/auth/sessions`). Admin and Manager only.
pub const SESSIONS_VIEW: &str = "sessions:view";
/// Opening, counting and closing a cashier's register shift. Not the login
/// sessions above: every role that takes payments needs this one.
pub const REGISTER_SESSIONS_VIEW: &str = "register_sessions:view";
pub const REGISTER_SESSIONS_MANAGE: &str = "register_sessions:manage";
pub const CASH_MOVEMENTS_CREATE: &str = "cash_movements:create";
pub const CASH_MOVEMENTS_VIEW: &str = "cash_movements:view";
/// Reversing a cash movement after the short undo window has passed. Manager and Admin. (The
/// person who recorded a movement can undo it themselves inside the window.)
pub const CASH_MOVEMENTS_REVERSE: &str = "cash_movements:reverse";
/// Rent and Bills, Staff Payment and Supplier Payment from the drawer: the
/// larger, accountable payouts only a Manager or Admin may record.
pub const CASH_MOVEMENTS_CREATE_MANAGED: &str = "cash_movements:create:managed";
/// Creating a drawer category on the spot. Every role holds it, but only for the
/// low-risk movement types; the rest need `CASH_CATEGORIES_MANAGE`.
pub const CASH_CATEGORIES_CREATE: &str = "cash_categories:create";
/// Creating a category for any movement type, and renaming, merging, hiding and
/// reviewing categories. Manager and Admin.
pub const CASH_CATEGORIES_MANAGE: &str = "cash_categories:manage";
/// Reading the expense ledger and its figures. Manager and Admin: a cashier sees the drawer's
/// own movements, not the shop's costs.
pub const EXPENSES_VIEW: &str = "expenses:view";
/// Entering a cost paid from the bank, a card or online (or a bill still to pay) and paying it.
pub const EXPENSES_MANAGE: &str = "expenses:manage";
/// Reading the income ledger (money received that is not a sale) and its figures. Manager and Admin.
/// Reading the account balances (drawer, bank, card, online). Manager and Admin.
pub const ACCOUNTS_VIEW: &str = "accounts:view";
/// Setting the opening balance of the bank, card and online accounts. Admin only: it moves what the shop believes it holds.
pub const ACCOUNTS_MANAGE: &str = "accounts:manage";
/// Reading what suppliers are owed and what staff still owe on advances. Manager and Admin.
pub const BALANCES_VIEW: &str = "balances:view";
/// Setting where a supplier balance starts and recording an advance recovered from a staff member's pay. Manager and Admin.
pub const BALANCES_MANAGE: &str = "balances:manage";
/// Reading the receipt photos attached to a movement, expense or income. Everyone who can see the record.
pub const ATTACHMENTS_VIEW: &str = "attachments:view";
/// Attaching a receipt photo to a movement, expense or income. Everyone who can record one.
pub const ATTACHMENTS_CREATE: &str = "attachments:create";
/// Changing the drawer rules: the undo window, the approval limit, the quick amounts and the float target. Admin only.
pub const CASH_SETTINGS_MANAGE: &str = "cash_settings:manage";
pub const INCOME_VIEW: &str = "income:view";
/// Entering money received into the bank, a card or online. Manager and Admin.
pub const INCOME_MANAGE: &str = "income:manage";
/// Adding landlords/vendors and other payees to the shared payee directory.
pub const PAYEES_MANAGE: &str = "payees:manage";

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

/// Create, list, print/reprint and cancel customer quotations (price offers,
/// not sales). Granted to Admin and Manager only — Staff tokens get a 403 from
/// every `/api/quotations` route, so hiding the UI is never the only guard.
pub const QUOTATIONS_MANAGE: &str = "quotations:manage";
