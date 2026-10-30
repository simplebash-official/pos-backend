use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::inventory::repository;
use axum::http::StatusCode;

/// 409s with `BARCODE_ALREADY_EXISTS` if another product already carries
/// `barcode`. Used both for a real, expected-to-sometimes-fire manual-entry
/// collision and as a defensive check after generation (which should never
/// actually fire, mirroring the SKU defensive check below).
pub(super) async fn ensure_barcode_available(db: &Db, barcode: &str) -> AppResult<()> {
    if repository::product::find_product_by_barcode(db, barcode)
        .await?
        .is_some()
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::BARCODE_ALREADY_EXISTS,
            format!("A product with barcode '{barcode}' already exists"),
        ));
    }
    Ok(())
}
