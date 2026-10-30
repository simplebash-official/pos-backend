use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::print_jobs::PrintJob;

use crate::modules::print_jobs::repository;

pub async fn delete_print_job(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.deleted", async move {
        let existing = repository::find_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
            })?;
        let object_id = existing
            .id
            .expect("persisted print job document must have an _id");

        let deleted = repository::delete(db, object_id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
            })?;
        Ok(deleted.into_print_job())
    })
    .await
}
