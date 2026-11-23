//! Dual-engine (Mongo / SQLite) access for the `login_sessions` audit log.
//! Visibility is `pub(crate)` so `service` can call in, but `mod repository;`
//! is private to `modules/auth`.

mod auth_repository;
mod password_reset_tokens_repository;

pub(crate) use auth_repository::count_login_sessions;
pub(crate) use auth_repository::insert_login_session;
pub(crate) use auth_repository::list_login_sessions;

pub(crate) use password_reset_tokens_repository::find_token_by_hash;
pub(crate) use password_reset_tokens_repository::insert_token;
pub(crate) use password_reset_tokens_repository::invalidate_active_tokens_for_user;
pub(crate) use password_reset_tokens_repository::mark_token_used;
