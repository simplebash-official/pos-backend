use super::ensure_valid_category::ensure_valid_category;
use super::resolve_display_names::resolve_display_names;
use super::validate_manual_barcode::validate_manual_barcode;
use super::validate_min_stock_threshold::validate_min_stock_threshold;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{BarcodeSource, Product, UpdateProductRequest};

use crate::modules::inventory::repository;
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Partial update — every field in `body` is optional, so each one falls
/// back to the existing document's value before the merged result is
/// re-validated (numbers, category) as if it were a fresh `create`.
pub(crate) async fn update_product(
    db: &Db,
    id: ObjectId,
    body: UpdateProductRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Product> {
    crate::core::logging::domain::tracked("inventory.product_updated", async move {
        let existing = repository::product::find_product_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            let mut conflicting = Vec::new();
            if let Some(ref name) = body.name
                && name != &existing.name
            {
                conflicting.push("name");
            }
            if let Some(ref cat) = body.category_key
                && cat != &existing.category_key
            {
                conflicting.push("categoryKey");
            }
            if let Some(ref subcat) = body.subcategory_key
                && subcat != &existing.subcategory_key
            {
                conflicting.push("subcategoryKey");
            }
            if let Some(min) = body.min_stock_threshold
                && min != existing.min_stock_threshold
            {
                conflicting.push("minStockThreshold");
            }
            if let Some(ref barcode) = body.barcode
                && Some(barcode) != existing.barcode.as_ref()
            {
                conflicting.push("barcode");
            }

            let (category_name, subcategory_name) =
                resolve_display_names(db, &existing.category_key, &existing.subcategory_key)
                    .await?;
            let server_doc = super::super::stock_batch::with_price_tiers(
                db,
                existing
                    .clone()
                    .into_product(category_name, subcategory_name),
            )
            .await?;

            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This product was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": server_doc,
                    "conflictingFields": conflicting,
                }),
            ));
        }

        if let Some(name) = &body.name
            && name.trim().is_empty()
        {
            return Err(AppError::validation("Product name cannot be empty"));
        }

        // Resolve a barcode edit before `existing` is partially moved below. A
        // value equal to the current one is a no-op (no needless write, no
        // `barcode_source` flip); a new value is validated and checked for a
        // collision against every *other* product.
        let barcode_change: Option<String> = match body.barcode {
            Some(ref candidate) if existing.barcode.as_deref() == Some(candidate.as_str()) => None,
            Some(candidate) => {
                validate_manual_barcode(&candidate)?;
                if repository::product::find_product_by_barcode_excluding(db, &candidate, id)
                    .await?
                    .is_some()
                {
                    return Err(AppError::custom(
                        StatusCode::CONFLICT,
                        codes::BARCODE_ALREADY_EXISTS,
                        format!("A product with barcode '{candidate}' already exists"),
                    ));
                }
                Some(candidate)
            }
            None => None,
        };

        let category_key = body.category_key.unwrap_or(existing.category_key);
        let subcategory_key = body.subcategory_key.unwrap_or(existing.subcategory_key);
        let min_stock_threshold = body
            .min_stock_threshold
            .unwrap_or(existing.min_stock_threshold);

        validate_min_stock_threshold(min_stock_threshold)?;
        let (category_name, subcategory_name) =
            ensure_valid_category(db, &category_key, &subcategory_key).await?;

        let mut set_doc = doc! {
            "category_key": &category_key,
            "subcategory_key": &subcategory_key,
            "min_stock_threshold": min_stock_threshold,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(name) = body.name {
            set_doc.insert("name", name);
        }
        if let Some(is_serialized) = body.is_serialized {
            set_doc.insert("is_serialized", is_serialized);
        }
        if let Some(warranty_months) = body.warranty_months {
            set_doc.insert("warranty_months", warranty_months);
        }
        if let Some(barcode) = barcode_change {
            set_doc.insert("barcode", barcode);
            // A staff-entered barcode is always "manual", even if the product
            // previously carried a system-generated one. Serialize through serde
            // so it stays in sync with `BarcodeSource`'s wire form.
            let source = mongodb::bson::serialize_to_bson(&BarcodeSource::Manual)
                .expect("BarcodeSource serializes to a plain string");
            set_doc.insert("barcode_source", source);
        }
        if let Some(device) = device_id {
            set_doc.insert("updated_by_device", device);
        }

        let updated = repository::product::update_product(db, id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;

        super::super::stock_batch::with_price_tiers(
            db,
            updated.into_product(category_name, subcategory_name),
        )
        .await
    })
    .await
}
