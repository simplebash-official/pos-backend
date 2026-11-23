//! The shop notifications a print job raises when it is ready and when it is handed back.

use crate::clients::db::Db;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::domain::notifications::{
    NewNotification, NotificationAudience, NotificationCategory, NotificationLink,
    NotificationSeverity, NotificationSource,
};
use crate::domain::print_jobs::PrintJob;
use crate::modules::notifications;

/// Tells everyone who works on print jobs that `job` just became ready or was handed back.
/// Any other status change, or a write that kept the status, raises nothing.
pub(super) async fn announce_status_change(
    db: &Db,
    job: &PrintJob,
    previous_status: &str,
) -> AppResult<()> {
    if job.status == previous_status {
        return Ok(());
    }
    let (severity, title, body) = match job.status.as_str() {
        "ready" => (
            NotificationSeverity::Success,
            "Print job ready",
            format!(
                "The {} order for {} ({}) is ready for pickup.",
                job.job_type, job.customer_name, job.ticket_number
            ),
        ),
        "delivered" => (
            NotificationSeverity::Info,
            "Print job handed back",
            format!(
                "The {} order ({}) was handed back to {}.",
                job.job_type, job.ticket_number, job.customer_name
            ),
        ),
        _ => return Ok(()),
    };
    notifications::service::publish(
        db,
        NewNotification {
            category: NotificationCategory::PrintJobs,
            severity,
            title: title.to_string(),
            body,
            link: Some(NotificationLink {
                path: format!("/print-jobs?jobKey={}", job.key),
                label: "Open job".to_string(),
            }),
            audience: NotificationAudience::Permission {
                permission: perm::PRINT_JOBS_WRITE.to_string(),
            },
            source: NotificationSource::Shop,
            dedupe_key: Some(format!("print-job:{}:{}", job.key, job.status)),
            raised_by_user_key: None,
        },
    )
    .await?;
    Ok(())
}
