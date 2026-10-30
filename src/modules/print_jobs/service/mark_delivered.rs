use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::print_jobs::PrintJob;

use crate::modules::print_jobs::repository;

/// Cross-module hook — called by `billing::service::sale::complete_sale`.
/// See `repairs::service::mark_delivered`'s comment for why this is
/// deliberately narrow (status only).
#[allow(dead_code)]
pub(crate) async fn mark_delivered(db: &Db, key: &str) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.delivered", async move {
        let updated = repository::set_status_by_key(db, key, "delivered")
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
            })?;
        Ok(updated.into_print_job())
    })
    .await
}
