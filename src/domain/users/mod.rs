//! Pure business types for user accounts and role management — no I/O, no
//! Mongo/Axum types beyond serde/utoipa derives. Mongo document shape lives
//! in `modules::users::model` (private) and converts into these before a
//! handler wraps them in `core::response::ApiResponse<T>`.

mod domain_users;
mod manager_approval;

pub use domain_users::CATALOG_SORT_IDS;
pub use domain_users::CreateUserRequest;
pub use domain_users::Role;
pub use domain_users::UpdateMyPreferencesRequest;
pub use domain_users::UpdateUserRequest;
pub use domain_users::User;
pub use domain_users::UserListQuery;
pub use domain_users::UserPreferences;
pub use domain_users::UsersResponse;
pub use manager_approval::ApprovedBy;
pub use manager_approval::ManagerApproval;
