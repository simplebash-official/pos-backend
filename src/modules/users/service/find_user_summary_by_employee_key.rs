use super::to_login_summary::to_login_summary;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::employees::EmployeeLoginSummary;

use crate::modules::users::repository;

/// Cross-module entry point `modules::employees::service` uses to answer
/// "does this employee already have a login" — on create/update validation
/// and on `delete_employee`'s `EMPLOYEE_HAS_LOGIN` guard.
pub(crate) async fn find_user_summary_by_employee_key(
    db: &Db,
    employee_key: &str,
) -> AppResult<Option<EmployeeLoginSummary>> {
    Ok(repository::find_user_by_employee_key(db, employee_key)
        .await?
        .map(to_login_summary))
}
