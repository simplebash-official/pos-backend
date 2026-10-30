use super::to_login_summary::to_login_summary;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::employees::EmployeeLoginSummary;

use crate::modules::users::repository;

use std::collections::HashMap;

/// Batch variant of `find_user_summary_by_employee_key` — used by
/// `modules::employees::service::list_employees`/`get_employees_by_keys`/
/// `hydrate_sync_documents` to enrich a whole page of employees with their
/// login summary in one query instead of one lookup per row.
pub(crate) async fn find_user_summaries_by_employee_keys(
    db: &Db,
    employee_keys: &[String],
) -> AppResult<HashMap<String, EmployeeLoginSummary>> {
    let documents = repository::find_users_by_employee_keys(db, employee_keys).await?;
    Ok(documents
        .into_iter()
        .filter_map(|document| {
            document
                .employee_key
                .clone()
                .map(|key| (key, to_login_summary(document)))
        })
        .collect())
}
