// Business rules for user account CRUD/role management: password
// hashing/verification, username uniqueness, login-credential checking, and
// the Admin/Manager management hierarchy (see `manageable_roles` below).
// Delegates all Mongo access to `super::repository`.

mod create_owner_admin_if_absent;
mod create_user;
mod create_user_for_caller;
mod delete_user;
mod ensure_manageable;
mod find_admin_document;
mod find_user_for_identity;
mod find_user_summaries_by_employee_keys;
mod find_user_summary_by_employee_key;
mod get_user;
mod get_user_for_caller;
mod hash_password;
mod list_users;
mod manageable_roles;
mod normalize_username;
mod reset_admin_credentials;
mod set_user_pin;
mod to_login_summary;
mod update_own_preferences;
mod update_user;
mod validate_owner_login;
mod validate_password_strength;
mod validate_username;
mod verify_credentials;
mod verify_password;
mod verify_user_pin;

pub use create_owner_admin_if_absent::create_owner_admin_if_absent;
pub use create_user::create_user;
pub(crate) use create_user_for_caller::create_user_for_caller;
pub(crate) use delete_user::delete_user;
pub(crate) use find_user_for_identity::find_user_for_identity;
pub(crate) use find_user_summaries_by_employee_keys::find_user_summaries_by_employee_keys;
pub(crate) use find_user_summary_by_employee_key::find_user_summary_by_employee_key;
pub(crate) use get_user::get_user;
pub(crate) use get_user_for_caller::get_user_for_caller;
pub(crate) use list_users::list_users;
pub use reset_admin_credentials::reset_admin_credentials;
pub use set_user_pin::set_user_pin;
pub(crate) use update_own_preferences::update_own_preferences;
pub(crate) use update_user::update_user;
pub use validate_owner_login::validate_owner_login;
pub(crate) use verify_credentials::verify_credentials;
pub use verify_user_pin::verify_user_pin;
