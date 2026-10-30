use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::employees::Employee;
use crate::modules::employees::repository;
use crate::modules::users;
use mongodb::bson::oid::ObjectId;

/// Refuses to delete (409 `EMPLOYEE_HAS_LOGIN`) while this employee still
/// has a linked login account — the login must be removed first, mirroring
/// `suppliers::service::delete_supplier`'s `SUPPLIER_HAS_PURCHASES` guard.
pub(crate) async fn delete_employee(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Employee> {
    crate::core::logging::domain::tracked("employees.deleted", async move {
        let existing = repository::find_employee_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
            })?;

        if users::service::find_user_summary_by_employee_key(db, &existing.key)
            .await?
            .is_some()
        {
            return Err(AppError::conflict(
                codes::EMPLOYEE_HAS_LOGIN,
                "This employee still has a login account. Remove their login first.",
            ));
        }

        let deleted = repository::delete_employee(db, id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
            })?;

        Ok(deleted.into_employee(None))
    })
    .await
}
