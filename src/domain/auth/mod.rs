// Pure business types for the auth feature — login, "who am I", and the
// login-session audit log. No I/O, no Mongo/Axum types beyond serde/utoipa
// derives. `auth` never defines its own user-shaped type: it reuses
// `domain::users::User` so the frontend has exactly one user shape to model.

mod login_request;
mod login_response;
mod login_session;
mod login_session_list_query;
mod login_sessions_response;
mod shop_lookup_response;

pub use login_request::LoginRequest;
pub use login_response::LoginResponse;
pub use login_session::LoginSession;
pub use login_session_list_query::LoginSessionListQuery;
pub use login_sessions_response::LoginSessionsResponse;
pub use shop_lookup_response::ShopLookupResponse;
