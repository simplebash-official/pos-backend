use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::employees::Employee;
use crate::modules::employees::repository;
use crate::modules::users;

/// Cross-module batch lookup — `reports::repository::commissions`' aggregation
/// calls this to resolve display name/role for a page of commission entries
/// keyed by `assignedEmployeeId`, in one query instead of one lookup per row
/// (the "reach another module through its service, never $lookup across a
/// module boundary" pattern `purchases`/`supplier_products` already use).
pub(crate) async fn get_employees_by_keys(db: &Db, keys: &[String]) -> AppResult<Vec<Employee>> {
    let documents = repository::find_employees_by_keys(db, keys).await?;
    let doc_keys: Vec<String> = documents.iter().map(|d| d.key.clone()).collect();
    let mut logins = users::service::find_user_summaries_by_employee_keys(db, &doc_keys).await?;

    Ok(documents
        .into_iter()
        .map(|document| {
            let login = logins.remove(&document.key);
            document.into_employee(login)
        })
        .collect())
}
