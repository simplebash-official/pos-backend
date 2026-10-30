//! Database access for the `users` collection/table. Functions here never
//! interpret a missing document as an error — they return `Option`/`Vec`
//! straight from the driver and leave the "not found" -> `AppError`
//! translation to `service`.

mod users_repository;

pub(crate) use users_repository::count_users_by_role;
pub(crate) use users_repository::delete_user;
pub(crate) use users_repository::find_user_by_employee_key;
pub(crate) use users_repository::find_user_by_id;
pub(crate) use users_repository::find_user_by_username;
pub(crate) use users_repository::find_users_by_employee_keys;
pub(crate) use users_repository::insert_user;
pub(crate) use users_repository::list_users;
pub(crate) use users_repository::update_user;
