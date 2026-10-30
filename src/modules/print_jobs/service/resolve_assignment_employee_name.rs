use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::employees;

/// See `repairs::service::resolve_assignment_employee_name` — identical
/// resolve-from-key/server-derives-name/never-trust-client-name rule.
pub(super) async fn resolve_assignment_employee_name(
    db: &Db,
    assigned_employee_id: Option<&str>,
) -> AppResult<Option<String>> {
    match assigned_employee_id {
        Some(key) => {
            let employee = employees::service::get_employee_by_key(db, key).await?;
            Ok(Some(employee.name))
        }
        None => Ok(None),
    }
}
