// Axum extractors that gate routes behind authentication/authorization.
// See `auth::CurrentUser` (any authenticated user, and its
// `require_permission` for gating by a specific permission) and
// `auth::AdminUser` (must additionally carry `role: Role::Admin`).
pub mod auth;
pub mod idempotency;
pub mod sync_headers;
pub mod timing;
