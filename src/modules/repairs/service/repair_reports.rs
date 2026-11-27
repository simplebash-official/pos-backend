//! What the warranty dashboard and report need to know about repair jobs: what free rework cost
//! the shop, and which technician did the job a warranty came from.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::utils::shop_day;
use crate::domain::job_reports::{JobTechnician, ReworkCost};
use crate::modules::repairs::repository;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// The rework jobs opened in `[start, end)` that were not cancelled, with their cost.
pub(crate) async fn rework_costs(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<ReworkCost>> {
    Ok(repository::rework_jobs_between(db, start, end)
        .await?
        .into_iter()
        .map(|job| ReworkCost {
            opened_on: shop_day(job.created_at.to_chrono()),
            cost_cents: job.cost_cents,
        })
        .collect())
}

/// Who each of these jobs was assigned to (`None`: nobody), by job key. A key with no job is
/// left out.
pub(crate) async fn technicians_by_job(
    db: &Db,
    keys: &[String],
) -> AppResult<HashMap<String, Option<JobTechnician>>> {
    let mut technicians = HashMap::new();
    for job in repository::find_by_keys(db, keys).await? {
        let technician = match (job.assigned_employee_id, job.assigned_employee_name) {
            (None, _) => None,
            (Some(employee_id), Some(name)) => Some(JobTechnician { employee_id, name }),
            (Some(_), None) => {
                return Err(AppError::internal(format!(
                    "repair `{}` has a technician without a name",
                    job.key
                )));
            }
        };
        technicians.insert(job.key, technician);
    }
    Ok(technicians)
}
