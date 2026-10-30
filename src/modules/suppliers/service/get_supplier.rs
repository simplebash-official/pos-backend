use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::Supplier;

use crate::modules::suppliers::repository;

use mongodb::bson::oid::ObjectId;

/// Fetch by id, 404ing with the module-specific `SUPPLIER_NOT_FOUND` code
/// rather than the generic `NOT_FOUND`.
pub(crate) async fn get_supplier(db: &Db, id: ObjectId) -> AppResult<Supplier> {
    let document = repository::find_supplier_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
        })?;

    Ok(document.into_supplier())
}
