// Axum extractors that gate routes behind authentication/authorization.
// See `auth::CurrentUser` (any authenticated user, and its
// `require_permission` for gating by a specific permission) and
// `auth::AdminUser` (must additionally carry `role: Role::Admin`).
pub mod auth;

// Cross-cutting request middleware not tied to auth. See
// `timing::add_processing_time_to_body` for per-request processing time.
pub mod timing;
