// Business rules for login, "who am I", and the login-session audit log.
// Owns no persistence of its own beyond the `login_sessions` collection
// (`super::repository`) — account data is always reached through
// `modules::users::service`, never a Mongo query of this module's own (see
// the module-level comment in `modules::auth::mod`).

mod list_sessions;
mod login;
mod lookup_shop;
mod me;
mod update_my_preferences;

pub(crate) use list_sessions::list_sessions;
pub(crate) use login::login;
pub(crate) use lookup_shop::lookup_shop;
pub(crate) use me::me;
pub(crate) use update_my_preferences::update_my_preferences;
