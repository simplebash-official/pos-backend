// Business logic for Technician & Staff Commissions report.

use mongodb::Database;

use super::dates;
use crate::{
    core::error::AppResult,
    domain::reports::{EmployeeCommissionsQuery, EmployeeCommissionsReportResponse},
    modules::reports::repository,
};

/// Retrieves the technician and staff job performance, profit generation, and earned commission splits.
pub(crate) async fn get_employee_commissions_report(
    db: &Database,
    query: EmployeeCommissionsQuery,
) -> AppResult<EmployeeCommissionsReportResponse> {
    let (start, end) = dates::parse_date_range(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    repository::commissions::aggregate_employee_commissions(
        db,
        start,
        end,
        query.employee_name.as_deref(),
    )
    .await
}
