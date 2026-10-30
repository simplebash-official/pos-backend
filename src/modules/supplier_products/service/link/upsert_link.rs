use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::supplier_products::{SupplierProductLink, UpsertSupplierProductLinkRequest};
use crate::modules::inventory::service::product as inventory_product;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;
use crate::modules::supplier_products::repository;
use crate::modules::suppliers::service as supplier_service;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Validates both `supplierKey` and `productKey` resolve to real documents,
/// then creates the link or, if `(supplierKey, productKey)` already has one,
/// updates its `costPriceCents`/`notes` in place rather than erroring on a
/// duplicate (matching the frontend's upsert contract for this endpoint).
pub async fn upsert_link(
    db: &Db,
    body: UpsertSupplierProductLinkRequest,
) -> AppResult<SupplierProductLink> {
    if let Some(cost_price_cents) = body.cost_price_cents
        && cost_price_cents < 0
    {
        return Err(AppError::validation("Cost price cannot be negative"));
    }

    supplier_service::get_supplier_by_key(db, &body.supplier_key).await?;
    inventory_product::get_product_by_key(db, &body.product_key).await?;

    let existing = repository::find_link(db, &body.supplier_key, &body.product_key).await?;

    let document = match existing {
        Some(_) => {
            let mut set_doc = doc! { "updated_at": BsonDateTime::now() };
            set_doc.insert("cost_price_cents", body.cost_price_cents);
            set_doc.insert("notes", body.notes);
            // `update_link` owns the `version` bump and the `deleted_at`
            // tombstone clear, matching `suppliers::repository::update_supplier`.
            repository::update_link(db, &body.supplier_key, &body.product_key, set_doc)
                .await?
                .expect("link existed moments ago, update_link must find it")
        }
        None => {
            let now = BsonDateTime::now();
            let document = SupplierProductLinkDocument {
                id: None,
                key: generate_id(prefixes::SUPPLIER_PRODUCT),
                supplier_key: body.supplier_key,
                product_key: body.product_key,
                cost_price_cents: body.cost_price_cents,
                notes: body.notes,
                version: 1,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                updated_by_device: None,
            };
            repository::insert_link(db, document).await?
        }
    };

    Ok(document.into_link())
}
