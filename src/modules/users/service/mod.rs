//! Business rules for user account CRUD/role management: password
//! hashing/verification, username uniqueness, login-credential checking, and
//! the Admin/Manager management hierarchy (see `manageable_roles` below).
//! Delegates all Mongo access to `super::repository`.

mod users_service;

pub use users_service::create_owner_admin_if_absent;
pub use users_service::create_user;
pub(crate) use users_service::create_user_for_caller;
pub(crate) use users_service::delete_user;
pub(crate) use users_service::find_user_for_identity;
pub(crate) use users_service::find_user_summaries_by_employee_keys;
pub(crate) use users_service::find_user_summary_by_employee_key;
pub(crate) use users_service::get_user;
pub(crate) use users_service::get_user_for_caller;
pub(crate) use users_service::list_users;
pub use users_service::reset_admin_credentials;
pub use users_service::set_user_pin;
pub(crate) use users_service::update_own_preferences;
pub(crate) use users_service::update_user;
pub use users_service::validate_owner_login;
pub(crate) use users_service::verify_credentials;
pub use users_service::verify_user_pin;
