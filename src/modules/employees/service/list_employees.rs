use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::core::utils::build_bson_regex;
use crate::domain::employees::{EmployeeListQuery, EmployeesResponse};
use crate::modules::employees::repository;
use crate::modules::users;
use mongodb::bson::{Document, doc};

/// Builds the Mongo filter from query params (free-text search across
/// name/phone/nicOrId, plus exact role/status filters) — same construction
/// style as `suppliers::service::list_suppliers`. Not paginated: a shop's
/// staff roster is small enough to return in full.
pub async fn list_employees(db: &Db, query: EmployeeListQuery) -> AppResult<EmployeesResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "phone": { "$regex": pattern.clone() } },
                { "nic_or_id": { "$regex": pattern } },
            ]
        });
    }
    if let Some(role) = query.role {
        and_clauses.push(doc! { "role": role.as_str() });
    }
    if let Some(status) = query.status {
        and_clauses.push(doc! { "status": status.as_str() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let documents = repository::list_employees(db, filter).await?;
    let keys: Vec<String> = documents.iter().map(|d| d.key.clone()).collect();
    let mut logins = users::service::find_user_summaries_by_employee_keys(db, &keys).await?;

    let employees = documents
        .into_iter()
        .map(|document| {
            let login = logins.remove(&document.key);
            document.into_employee(login)
        })
        .collect();

    Ok(EmployeesResponse { employees })
}
