use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::employees::Employee;
use crate::modules::employees::repository;
use crate::modules::users;
use mongodb::bson::oid::ObjectId;

/// Fetch by id, 404ing with the module-specific `EMPLOYEE_NOT_FOUND` code
/// rather than the generic `NOT_FOUND`.
pub(crate) async fn get_employee(db: &Db, id: ObjectId) -> AppResult<Employee> {
    let document = repository::find_employee_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;
    let login = users::service::find_user_summary_by_employee_key(db, &document.key).await?;

    Ok(document.into_employee(login))
}
