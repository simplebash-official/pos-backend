use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::repairs::Repair;

use crate::modules::repairs::repository;

pub async fn get_repair(db: &Db, id_or_key: &str) -> AppResult<Repair> {
    let document = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    Ok(document.into_repair())
}
