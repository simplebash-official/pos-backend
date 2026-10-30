use super::ensure_barcode_available::ensure_barcode_available;
use super::resolve_display_names::resolve_display_names;

use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{BarcodeSource, Product};

use crate::modules::inventory::repository;
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Gives a product that has no barcode a generated shop barcode, numbered
/// exactly like `autoGenerateBarcode` on create. Refused (409
/// `PRODUCT_HAS_BARCODE`) when the product already has one, so a barcode that
/// may already be printed on stickers can never change underneath them.
pub(crate) async fn generate_barcode(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Product> {
    crate::core::logging::domain::tracked("inventory.product_barcode_generated", async move {
        let existing = repository::product::find_product_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;
        if existing.barcode.is_some() {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::PRODUCT_HAS_BARCODE,
                format!("'{}' already has a barcode", existing.name),
            ));
        }

        let generated = crate::modules::barcode::service::generator::generate(
            db,
            crate::modules::barcode::service::generator::namespaces::PRODUCT,
        )
        .await?;
        ensure_barcode_available(db, &generated).await?;

        let source = mongodb::bson::serialize_to_bson(&BarcodeSource::Generated)
            .expect("BarcodeSource serializes to a plain string");
        let mut set_doc = doc! {
            "barcode": &generated,
            "barcode_source": source,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(device) = device_id {
            set_doc.insert("updated_by_device", device);
        }

        let updated = repository::product::update_product(db, id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;
        let (category_name, subcategory_name) =
            resolve_display_names(db, &updated.category_key, &updated.subcategory_key).await?;
        super::super::stock_batch::with_price_tiers(
            db,
            updated.into_product(category_name, subcategory_name),
        )
        .await
    })
    .await
}
