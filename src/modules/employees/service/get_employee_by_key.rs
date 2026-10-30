use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::employees::Employee;
use crate::modules::employees::repository;
use crate::modules::users;

/// Same as `get_employee`, looked up by `key` instead of `ObjectId` — the
/// cross-module entry point `repairs`/`print_jobs` call to resolve an
/// `assignedEmployeeId` and derive its display name server-side.
pub(crate) async fn get_employee_by_key(db: &Db, key: &str) -> AppResult<Employee> {
    let document = repository::find_employee_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;
    let login = users::service::find_user_summary_by_employee_key(db, &document.key).await?;

    Ok(document.into_employee(login))
}
