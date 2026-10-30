// Database access for the `users` collection/table. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`.

mod count_users_by_role;
mod delete_user;
mod find_user_by_employee_key;
mod find_user_by_id;
mod find_user_by_username;
mod find_users_by_employee_keys;
mod insert_user;
mod list_users;
mod update_user;
mod user_from_sqlite_row;
mod users;

pub(crate) use count_users_by_role::count_users_by_role;
pub(crate) use delete_user::delete_user;
pub(crate) use find_user_by_employee_key::find_user_by_employee_key;
pub(crate) use find_user_by_id::find_user_by_id;
pub(crate) use find_user_by_username::find_user_by_username;
pub(crate) use find_users_by_employee_keys::find_users_by_employee_keys;
pub(crate) use insert_user::insert_user;
pub(crate) use list_users::list_users;
pub(crate) use update_user::update_user;
