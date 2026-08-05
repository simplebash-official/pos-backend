//! Role -> default permission set. Hardcoded, no DB-backed custom roles or
//! per-user overrides — looked up once at login (`modules::auth::service::login`)
//! and embedded into the JWT's `permissions` claim, so route-level
//! `require_permission` checks never need a database round trip. A role
//! change on a user's account only takes effect on that user's *next*
//! login, since there is no revocation infra to force-refresh a live token.

use crate::{core::constants::permissions as perm, domain::users::Role};

const ADMIN_PERMISSIONS: &[&str] = &[
    perm::USERS_MANAGE,
    perm::SESSIONS_VIEW,
    perm::INVENTORY_READ,
    perm::INVENTORY_WRITE,
    perm::INVENTORY_ADMIN,
    perm::BILLING_WRITE,
    perm::REPAIRS_WRITE,
    perm::PRINT_JOBS_WRITE,
    perm::REPORTS_VIEW,
    perm::CUSTOMERS_WRITE,
];

// Everything an Admin has except full account management — a Manager runs
// day-to-day shop operations and may provision Staff accounts specifically
// (`USERS_MANAGE_STAFF`), but not Manager or Admin accounts (see
// `modules::users::service`'s `manageable_roles`).
const MANAGER_PERMISSIONS: &[&str] = &[
    perm::USERS_MANAGE_STAFF,
    perm::SESSIONS_VIEW,
    perm::INVENTORY_READ,
    perm::INVENTORY_WRITE,
    perm::INVENTORY_ADMIN,
    perm::BILLING_WRITE,
    perm::REPAIRS_WRITE,
    perm::PRINT_JOBS_WRITE,
    perm::REPORTS_VIEW,
    perm::CUSTOMERS_WRITE,
];

// Day-to-day operational permissions only — no inventory administration, no
// supplier/purchase management, no report or session visibility.
const STAFF_PERMISSIONS: &[&str] = &[
    perm::INVENTORY_READ,
    perm::BILLING_WRITE,
    perm::REPAIRS_WRITE,
    perm::PRINT_JOBS_WRITE,
    perm::CUSTOMERS_WRITE,
];

/// The fixed permission set a role's JWT carries at login. Three flat
/// slices rather than a computed union/subtraction, so each role's actual
/// grant is visible at a glance — extending a role means editing its slice
/// directly, not tracing through a derivation.
pub fn default_permissions(role: Role) -> &'static [&'static str] {
    match role {
        Role::Admin => ADMIN_PERMISSIONS,
        Role::Manager => MANAGER_PERMISSIONS,
        Role::Staff => STAFF_PERMISSIONS,
    }
}
