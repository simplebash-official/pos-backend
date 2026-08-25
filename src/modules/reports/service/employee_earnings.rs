// Business logic for one employee's itemized commission history.

use mongodb::Database;

use super::dates;
use crate::{
    core::error::AppResult,
    domain::{employees::EmployeeEarningsResponse, reports::EmployeeCommissionsQuery},
    modules::{employees, reports::repository},
};

/// Retrieves the itemized (non-aggregated) commission line items for one
/// employee. 404s via `employees::service::get_employee_by_key` if the key
/// doesn't resolve, before doing any repair/print-job lookups.
pub(crate) async fn get_employee_earnings(
    db: &Database,
    employee_key: &str,
    query: EmployeeCommissionsQuery,
) -> AppResult<EmployeeEarningsResponse> {
    employees::service::get_employee_by_key(db, employee_key).await?;

    let (start, end) = dates::parse_date_range(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    let records =
        repository::employee_earnings::list_employee_earnings(db, employee_key, start, end).await?;

    Ok(EmployeeEarningsResponse { records })
}
