use chrono::Utc;

/// Statuses that mean the job is finished and off the shop's plate, so a
/// passed promised-ready date no longer matters.
const CLOSED_JOB_STATUSES: [&str; 2] = ["delivered", "cancelled"];

/// Whether a repair/print job should be flagged overdue: still open and its
/// `promised_ready_at` date has passed. Pure function of the two persisted
/// fields, recomputed on every read (`RepairDocument::into_repair` /
/// `PrintJobDocument::into_print_job`) — mirrors `billing::compute_is_overdue`.
#[must_use]
pub fn compute_job_is_overdue(status: &str, promised_ready_at: Option<&str>) -> bool {
    if CLOSED_JOB_STATUSES.contains(&status) {
        return false;
    }
    let Some(promised) = promised_ready_at else {
        return false;
    };
    match chrono::NaiveDate::parse_from_str(promised, "%Y-%m-%d") {
        Ok(due) => due < Utc::now().date_naive(),
        Err(_) => false,
    }
}
