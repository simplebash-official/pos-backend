use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::repairs::Repair;

use crate::modules::repairs::repository;

/// Cross-module hook — called by `billing::service::sale::complete_sale`
/// when a cart line bills a repair ticket. Narrow on purpose: only flips
/// `status`, never touches cost/assignment fields, since a sale completing
/// shouldn't silently rewrite ticket details a technician entered.
#[allow(dead_code)]
pub(crate) async fn mark_delivered(db: &Db, key: &str) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.delivered", async move {
        let updated = repository::set_status_by_key(db, key, "delivered")
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?;
        Ok(updated.into_repair())
    })
    .await
}
