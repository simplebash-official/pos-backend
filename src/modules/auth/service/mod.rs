//! Business rules for login, "who am I", and the login-session audit log.
//! Owns no persistence of its own beyond the `login_sessions` collection
//! (`super::repository`) — account data is always reached through
//! `modules::users::service`, never a Mongo query of this module's own (see
//! the module-level comment in `modules::auth::mod`).

mod auth_service;

pub(crate) use auth_service::list_sessions;
pub(crate) use auth_service::login;
pub(crate) use auth_service::lookup_shop;
pub(crate) use auth_service::me;
pub(crate) use auth_service::update_my_preferences;
