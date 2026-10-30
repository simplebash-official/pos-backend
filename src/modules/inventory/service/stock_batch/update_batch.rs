use super::batch_not_found::batch_not_found;
use super::validate_batch_prices::validate_batch_prices;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{StockBatch, UpdateStockBatchRequest};
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;

/// Edits a batch's selling price and/or cost (`PATCH /batches/{key}`).
/// Only an Admin may change the cost; the caller passes `may_edit_cost`.
pub(crate) async fn update_batch(
    db: &Db,
    key: &str,
    body: UpdateStockBatchRequest,
    may_edit_cost: bool,
) -> AppResult<StockBatch> {
    crate::core::logging::domain::tracked("inventory.stock_batch_updated", async move {
        if body.selling_price_cents.is_none() && body.unit_cost_cents.is_none() {
            return Err(AppError::validation(
                "Send sellingPriceCents and/or unitCostCents to change",
            ));
        }
        if body.unit_cost_cents.is_some() && !may_edit_cost {
            return Err(AppError::forbidden_with_code(
                "Only an Admin can change a batch's cost",
                codes::ADMIN_REQUIRED,
            ));
        }
        let existing = repository::stock_batch::find_batch_by_key(db, key)
            .await?
            .ok_or_else(|| batch_not_found(key))?;
        validate_batch_prices(
            body.unit_cost_cents.unwrap_or(existing.unit_cost_cents),
            body.selling_price_cents
                .unwrap_or(existing.selling_price_cents),
        )?;
        repository::stock_batch::update_batch(
            db,
            key,
            body.selling_price_cents,
            body.unit_cost_cents,
        )
        .await?
        .map(StockBatchDocument::into_stock_batch)
        .ok_or_else(|| batch_not_found(key))
    })
    .await
}
