//! Role -> default permission set. Hardcoded, no DB-backed custom roles or
//! per-user overrides — looked up once at login (`modules::auth::service::login`)
//! and embedded into the JWT's `permissions` claim, so route-level
//! `require_permission` checks never need a database round trip. A role
//! change on a user's account only takes effect on that user's *next*
//! login, since there is no revocation infra to force-refresh a live token.

mod constants_roles;

pub use constants_roles::ADMIN_USERNAME;
pub use constants_roles::default_permissions;
