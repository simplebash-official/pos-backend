use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::repairs::Repair;

use crate::modules::repairs::repository;

pub async fn delete_repair(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.deleted", async move {
        let existing = repository::find_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?;
        let object_id = existing
            .id
            .expect("persisted repair document must have an _id");

        let deleted = repository::delete(db, object_id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?;
        Ok(deleted.into_repair())
    })
    .await
}
