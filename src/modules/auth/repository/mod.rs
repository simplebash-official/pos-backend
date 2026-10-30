//! Dual-engine (Mongo / SQLite) access for the `login_sessions` audit log.
//! Visibility is `pub(crate)` so `service` can call in, but `mod repository;`
//! is private to `modules/auth`.

mod auth_repository;

pub(crate) use auth_repository::count_login_sessions;
pub(crate) use auth_repository::insert_login_session;
pub(crate) use auth_repository::list_login_sessions;
