use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::employees;

/// When `assigned_employee_id` is present, it's resolved against the
/// `employees` collection and the display name is server-derived from that
/// record — a client-sent `assigned_employee_name` is never trusted once a
/// valid id is given, same "resolved from a key, don't trust a client-sent
/// duplicate" rule `billing::service::sale` applies to a sale item's
/// `productKey`/`assignedEmployeeName`. An unresolvable id 404s
/// (`EMPLOYEE_NOT_FOUND`) before any write. Absent `assigned_employee_id`
/// leaves the ticket unassigned — no employee module lookup happens.
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
