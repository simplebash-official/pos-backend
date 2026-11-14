//! Business rules for user account CRUD/role management: password
//! hashing/verification, username uniqueness, login-credential checking, and
//! the Admin/Manager management hierarchy (see `manageable_roles` below).
//! Delegates all Mongo access to `super::repository`.

mod users_service;

pub(crate) use users_service::AccountStatus;
pub(crate) use users_service::account_status;
pub use users_service::create_owner_admin_if_absent;
pub use users_service::create_user;
pub(crate) use users_service::create_user_for_caller;
pub(crate) use users_service::delete_user;
pub use users_service::find_admin_document;
pub(crate) use users_service::find_user_for_identity;
pub(crate) use users_service::find_user_summaries_by_employee_keys;
pub(crate) use users_service::find_user_summary_by_employee_key;
pub(crate) use users_service::fingerprint_for_login;
pub(crate) use users_service::get_user;
pub(crate) use users_service::get_user_for_caller;
pub use users_service::hash_password;
pub(crate) use users_service::list_users;
pub(crate) use users_service::pin_approver;
pub use users_service::reset_admin_credentials;
pub(crate) use users_service::resolve_approval;
pub use users_service::set_user_pin;
pub use users_service::update_admin_password_and_bump_version;
pub(crate) use users_service::update_own_preferences;
pub(crate) use users_service::update_user;
pub use users_service::validate_owner_login;
pub use users_service::validate_password_strength;
pub(crate) use users_service::verify_credentials;
pub use users_service::verify_user_pin;
