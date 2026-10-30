use super::one_movement::one_movement;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::sessions::CashMovement;

use crate::modules::sessions::repository::movements;

pub async fn get_movement(db: &Db, key: &str) -> AppResult<CashMovement> {
    let doc = movements::find_cash_movement_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Cash movement not found", codes::CASH_MOVEMENT_NOT_FOUND)
        })?;
    one_movement(db, doc).await
}
