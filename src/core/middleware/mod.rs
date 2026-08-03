// Axum extractors that gate routes behind authentication/authorization.
// See `auth::CurrentUser` (any authenticated user) and `auth::AdminUser`
// (must additionally carry `role: "admin"`).
pub mod auth;
