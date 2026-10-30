use super::advance_summary::advance_summary;
use super::advances_given::advances_given;
use super::given::Given;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::balances::{BalanceSubject, StaffAdvancesResponse};
use crate::domain::employees::EmployeeListQuery;
use crate::modules::balances::model::{AdvanceRecoveryDocument, BalanceOpeningDocument};
use crate::modules::balances::repository;
use crate::modules::employees;
use std::collections::HashMap;

/// Everyone who has been given an advance (or has an opening or a recovery), with what they still owe.
pub async fn list_staff_advances(db: &Db) -> AppResult<StaffAdvancesResponse> {
    let given = advances_given(db).await?;
    let recoveries = repository::list_recoveries(db).await?;
    let openings: HashMap<String, BalanceOpeningDocument> =
        repository::list_openings(db, BalanceSubject::Staff)
            .await?
            .into_iter()
            .map(|o| (o.subject_key.clone(), o))
            .collect();
    let mut staff = Vec::new();
    for e in employees::service::list_employees(db, EmployeeListQuery::default())
        .await?
        .employees
    {
        let g: Vec<&Given> = given.iter().filter(|g| g.employee_key == e.key).collect();
        let r: Vec<&AdvanceRecoveryDocument> = recoveries
            .iter()
            .filter(|r| r.employee_key == e.key)
            .collect();
        let opening = openings.get(&e.key);
        if g.is_empty() && r.is_empty() && opening.is_none() {
            continue;
        }
        staff.push(advance_summary(&e.key, e.name, opening, &g, &r));
    }
    staff.sort_by(|a, b| {
        b.outstanding_cents
            .cmp(&a.outstanding_cents)
            .then(a.name.cmp(&b.name))
    });
    Ok(StaffAdvancesResponse { staff })
}
