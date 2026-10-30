use super::delete_employee::delete_employee;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use mongodb::bson::oid::ObjectId;

/// Batch delete for `DELETE /employees/batch`. Ids that aren't valid
/// `ObjectId`s, that don't resolve to an existing employee, or that are
/// blocked by the `EMPLOYEE_HAS_LOGIN` guard are silently skipped rather
/// than failing the whole request — same "valid ones still succeed"
/// semantics as `suppliers::service::delete_suppliers`.
pub(crate) async fn delete_employees(db: &Db, ids: Vec<String>) -> AppResult<u64> {
    crate::core::logging::domain::tracked("employees.bulk_deleted", async move {
        let mut deleted_count = 0u64;
        for id in ids {
            if let Ok(object_id) = ObjectId::parse_str(&id)
                && delete_employee(db, object_id, None).await.is_ok()
            {
                deleted_count += 1;
            }
        }
        Ok(deleted_count)
    })
    .await
}
