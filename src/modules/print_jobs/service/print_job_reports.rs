//! What the warranty dashboard and report need to know about print jobs: what free rework cost
//! the shop, and who did the job a warranty came from and which line it covers.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::utils::shop_day;
use crate::domain::job_reports::{JobTechnician, PrintJobFacts, ReworkCost};
use crate::modules::print_jobs::repository;
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

/// Who each of these jobs was assigned to and what its lines are, by job key. A key with no job
/// is left out.
pub(crate) async fn facts_by_job(
    db: &Db,
    keys: &[String],
) -> AppResult<HashMap<String, PrintJobFacts>> {
    let mut facts = HashMap::new();
    for job in repository::find_by_keys(db, keys).await? {
        let technician = match (job.assigned_employee_id, job.assigned_employee_name) {
            (None, _) => None,
            (Some(employee_id), Some(name)) => Some(JobTechnician { employee_id, name }),
            (Some(_), None) => {
                return Err(AppError::internal(format!(
                    "print job `{}` has a technician without a name",
                    job.key
                )));
            }
        };
        let line_descriptions = job
            .items
            .into_iter()
            .map(|item| (item.key, item.description))
            .collect();
        facts.insert(
            job.key,
            PrintJobFacts {
                technician,
                line_descriptions,
            },
        );
    }
    Ok(facts)
}
