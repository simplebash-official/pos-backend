// Dual-engine (Mongo / SQLite) access for the `login_sessions` audit log.
// Visibility is `pub(crate)` so `service` can call in, but `mod repository;`
// is private to `modules/auth`.

mod count_login_sessions;
mod insert_login_session;
mod list_login_sessions;
mod login_sessions;

pub(crate) use count_login_sessions::count_login_sessions;
pub(crate) use insert_login_session::insert_login_session;
pub(crate) use list_login_sessions::list_login_sessions;
