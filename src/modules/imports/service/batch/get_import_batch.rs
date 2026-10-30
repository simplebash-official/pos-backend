use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::imports::ImportBatch;
use crate::modules::imports::repository;

pub(crate) async fn get_import_batch(db: &Db, key: &str) -> AppResult<ImportBatch> {
    let doc = repository::batch::find_batch_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Import batch not found", codes::IMPORT_BATCH_NOT_FOUND)
        })?;

    Ok(doc.into_import_batch())
}
