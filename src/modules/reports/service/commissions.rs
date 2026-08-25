// Business logic for the Technician & Staff Commissions report. Resolves
// each bucket `repository::commissions::aggregate_employee_commissions`
// returns (keyed by `assigned_employee_id`, or an unassigned sentinel)
// against `modules::employees` in a single batch call — this is where the
// cross-module lookup belongs, not the repository (see that module's
// comment for the "reach through service, never repository" rule).

use std::collections::HashMap;

use mongodb::Database;

use super::dates;
use crate::{
    core::error::AppResult,
    domain::reports::{
        EmployeeCommissionsQuery, EmployeeCommissionsReportResponse, EmployeePerformanceEntry,
    },
    modules::{
        employees,
        reports::repository::{
            self,
            commissions::{UNASSIGNED_PRINTER, UNASSIGNED_TECHNICIAN},
        },
    },
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

    let mut perf_map = repository::commissions::aggregate_employee_commissions(
        db,
        start,
        end,
        query.employee_key.as_deref(),
    )
    .await?;

    // Resolve every real employee bucket's display name/role in one batch
    // call — never a $lookup (see repository module comment).
    let employee_keys: Vec<String> = perf_map
        .keys()
        .filter(|k| k.as_str() != UNASSIGNED_TECHNICIAN && k.as_str() != UNASSIGNED_PRINTER)
        .cloned()
        .collect();
    let resolved = employees::service::get_employees_by_keys(db, &employee_keys).await?;
    let resolved_by_key: HashMap<String, (String, String)> = resolved
        .into_iter()
        .map(|employee| {
            (
                employee.key,
                (employee.name, employee.role.as_str().to_string()),
            )
        })
        .collect();

    for (bucket_key, entry) in perf_map.iter_mut() {
        if bucket_key == UNASSIGNED_TECHNICIAN || bucket_key == UNASSIGNED_PRINTER {
            continue;
        }
        match resolved_by_key.get(bucket_key) {
            Some((name, role)) => {
                entry.employee_name = name.clone();
                entry.role = role.clone();
            }
            // The employee record was deleted after these jobs were
            // completed — graceful degradation, never a hard error, same
            // spirit as `Purchase`'s optional live-resolved supplier/product.
            None => entry.employee_name = "Removed Employee".to_string(),
        }
    }

    let mut total_commissions_cents = 0;
    let mut total_revenue_cents = 0;
    let mut total_jobs_count = 0;

    let employees: Vec<EmployeePerformanceEntry> = perf_map.into_values().collect();
    for emp in &employees {
        total_commissions_cents += emp.earned_commission_cents;
        total_revenue_cents += emp.revenue_generated_cents;
        total_jobs_count += emp.assigned_jobs_count;
    }

    Ok(EmployeeCommissionsReportResponse {
        employees,
        total_commissions_cents,
        total_revenue_cents,
        total_jobs_count,
        period_start: start,
        period_end: end,
    })
}
