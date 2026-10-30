use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::StockMovementsResponse;
use crate::modules::inventory::model::StockMovementDocument;
use crate::modules::inventory::repository;
use mongodb::bson::oid::ObjectId;

/// The audit-trail history for one product. 404s if the product itself
/// doesn't exist (rather than just returning an empty list), so a typo'd
/// id is distinguishable from a real product with no movements yet.
pub(crate) async fn product_movements(db: &Db, id: ObjectId) -> AppResult<StockMovementsResponse> {
    if repository::product::find_product_by_id(db, id)
        .await?
        .is_none()
    {
        return Err(AppError::not_found_with_code(
            "Product not found",
            codes::PRODUCT_NOT_FOUND,
        ));
    }

    let documents = repository::stock::find_stock_movements_for_product(db, id).await?;
    let movements = documents
        .into_iter()
        .map(StockMovementDocument::into_stock_movement)
        .collect();

    Ok(StockMovementsResponse { movements })
}
