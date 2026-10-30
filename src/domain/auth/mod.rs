//! Pure business types for the auth feature — login, "who am I", and the
//! login-session audit log. No I/O, no Mongo/Axum types beyond serde/utoipa
//! derives. `auth` never defines its own user-shaped type: it reuses
//! `domain::users::User` so the frontend has exactly one user shape to model.

mod domain_auth;

pub use domain_auth::LoginRequest;
pub use domain_auth::LoginResponse;
pub use domain_auth::LoginSession;
pub use domain_auth::LoginSessionListQuery;
pub use domain_auth::LoginSessionsResponse;
pub use domain_auth::ShopLookupResponse;
