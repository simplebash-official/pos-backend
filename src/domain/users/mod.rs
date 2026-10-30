// Pure business types for user accounts and role management — no I/O, no
// Mongo/Axum types beyond serde/utoipa derives. Mongo document shape lives
// in `modules::users::model` (private) and converts into these before a
// handler wraps them in `core::response::ApiResponse<T>`.

mod catalog_sort_ids;
mod create_user_request;
mod role;
mod update_my_preferences_request;
mod update_user_request;
mod user;
mod user_list_query;
mod user_preferences;
mod users_response;

pub use catalog_sort_ids::CATALOG_SORT_IDS;
pub use create_user_request::CreateUserRequest;
pub use role::Role;
pub use update_my_preferences_request::UpdateMyPreferencesRequest;
pub use update_user_request::UpdateUserRequest;
pub use user::User;
pub use user_list_query::UserListQuery;
pub use user_preferences::UserPreferences;
pub use users_response::UsersResponse;
