use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::print_jobs::PrintJob;

use crate::modules::print_jobs::repository;

pub async fn get_print_job(db: &Db, id_or_key: &str) -> AppResult<PrintJob> {
    let document = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    Ok(document.into_print_job())
}
