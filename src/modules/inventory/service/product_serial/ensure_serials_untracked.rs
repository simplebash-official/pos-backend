use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::inventory::repository;

/// Fails with `SERIAL_ALREADY_EXISTS` if any of `serial_numbers` is already
/// tracked (across ANY product — serials are globally unique). A purchase
/// calls this before writing anything, so a duplicate stops it cleanly.
pub(crate) async fn ensure_serials_untracked(db: &Db, serial_numbers: &[String]) -> AppResult<()> {
    for serial_number in serial_numbers {
        if repository::product_serial::find_serial_by_number(db, serial_number)
            .await?
            .is_some()
        {
            return Err(AppError::custom(
                axum::http::StatusCode::CONFLICT,
                codes::SERIAL_ALREADY_EXISTS,
                format!("Serial number '{serial_number}' is already tracked"),
            ));
        }
    }
    Ok(())
}
