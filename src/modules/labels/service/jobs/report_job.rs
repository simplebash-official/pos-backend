use super::job_not_found::job_not_found;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::labels::{LabelJob, LabelJobOutcome, UpdateLabelJobRequest};

use crate::modules::labels::repository;
use axum::http::StatusCode;

const JOB_ALREADY_REPORTED: &str = "LABEL_JOB_ALREADY_REPORTED";

const MAX_ERROR_CHARS: usize = 500;

/// Records what the printer step did with a job that is still waiting
/// (`rendered`): `sent` or `failed` with the reason.
pub(crate) async fn report_job(
    db: &Db,
    key: &str,
    request: UpdateLabelJobRequest,
) -> AppResult<LabelJob> {
    crate::core::logging::domain::tracked("labels.job_reported", async move {
        let error = request.error.as_deref().map(str::trim);
        match (request.outcome, error) {
            (LabelJobOutcome::Rendered, _) => {
                return Err(AppError::validation(
                    "Report whether the labels were sent or failed.",
                ));
            }
            (LabelJobOutcome::Sent, Some(_)) => {
                return Err(AppError::validation(
                    "Only a failed print carries a reason.",
                ));
            }
            (LabelJobOutcome::Failed, Some(reason))
                if reason.is_empty() || reason.chars().count() > MAX_ERROR_CHARS =>
            {
                return Err(AppError::validation(
                    "The reason must be 1 to 500 characters.",
                ));
            }
            _ => {}
        }
        if let Some(job) = repository::report_job_outcome(db, key, request.outcome, error).await? {
            return Ok(job.into_job());
        }
        match repository::find_job_by_key(db, key).await? {
            None => Err(job_not_found()),
            Some(_) => Err(AppError::custom(
                StatusCode::CONFLICT,
                JOB_ALREADY_REPORTED,
                "This print has already been reported.",
            )),
        }
    })
    .await
}
