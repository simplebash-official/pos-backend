use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{
    StockMovementListQuery, StockMovementListResponse, StockMovementType,
};
use crate::modules::inventory::model::StockMovementDocument;
use crate::modules::inventory::repository;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

/// Unfiltered, paginated listing of all stock movements for delta/offline sync.
pub(crate) async fn list_stock_movements(
    db: &Db,
    query: StockMovementListQuery,
) -> AppResult<StockMovementListResponse> {
    let mut filter = doc! { "deleted_at": { "$exists": false } };

    if let Some(ref pid_str) = query.product_id
        && !pid_str.is_empty()
    {
        let pid =
            ObjectId::parse_str(pid_str).map_err(|_| AppError::validation("Invalid product ID"))?;
        filter.insert("product_id", pid);
    }

    if let Some(mtype) = query.movement_type {
        let mtype_str = match mtype {
            StockMovementType::Sale => "sale",
            StockMovementType::PurchaseReceipt => "purchase_receipt",
            StockMovementType::RepairPartConsumption => "repair_part_consumption",
            StockMovementType::ManualAdjustment => "manual_adjustment",
            StockMovementType::InvoiceVoidReversal => "invoice_void_reversal",
            StockMovementType::ReturnRestock => "return_restock",
            StockMovementType::ReturnWriteOff => "return_write_off",
            StockMovementType::ReturnSupplierRma => "return_supplier_rma",
            StockMovementType::OpeningBalance => "opening_balance",
        };
        filter.insert("movement_type", mtype_str);
    }

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 500);
    let skip = (page - 1) * limit;

    let (documents, total) =
        repository::stock::list_stock_movements_paginated(db, filter, skip, limit as i64).await?;

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };

    let items = documents
        .into_iter()
        .map(StockMovementDocument::into_stock_movement)
        .collect();

    Ok(StockMovementListResponse {
        items,
        pagination: crate::domain::inventory::PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        },
    })
}
