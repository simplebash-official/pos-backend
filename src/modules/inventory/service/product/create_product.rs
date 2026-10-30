use super::ensure_barcode_available::ensure_barcode_available;
use super::ensure_valid_category::ensure_valid_category;
use super::validate_manual_barcode::validate_manual_barcode;
use super::validate_min_stock_threshold::validate_min_stock_threshold;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::inventory::{
    BarcodeSource, BatchSource, CreateProductRequest, Product, StockMovementType,
};
use crate::modules::inventory::model::ProductDocument;
use crate::modules::inventory::repository;
use axum::http::StatusCode;

use mongodb::bson::DateTime as BsonDateTime;

/// Resolves what (if anything) `create_product` should persist for
/// `barcode`/`barcode_source`, enforcing that a manual value and
/// auto-generation are mutually exclusive. A product supplying neither is fine:
/// a barcode can be added later, by hand (`update_product`) or generated
/// (`generate_barcode`).
async fn resolve_barcode(
    db: &Db,
    barcode: Option<String>,
    auto_generate_barcode: bool,
) -> AppResult<(Option<String>, Option<BarcodeSource>)> {
    match (barcode, auto_generate_barcode) {
        (Some(_), true) => Err(AppError::validation(
            "Provide either 'barcode' or 'autoGenerateBarcode', not both",
        )),
        (Some(manual), false) => {
            validate_manual_barcode(&manual)?;
            ensure_barcode_available(db, &manual).await?;
            Ok((Some(manual), Some(BarcodeSource::Manual)))
        }
        (None, true) => {
            let generated = crate::modules::barcode::service::generator::generate(
                db,
                crate::modules::barcode::service::generator::namespaces::PRODUCT,
            )
            .await?;
            // Defensive fallback only — the atomic namespaced counter plus
            // fixed reserved prefix make a collision structurally
            // impossible, same "should never actually fire" reasoning as
            // the SKU defensive check below.
            ensure_barcode_available(db, &generated).await?;
            Ok((Some(generated), Some(BarcodeSource::Generated)))
        }
        (None, false) => Ok((None, None)),
    }
}

/// Validates required fields, price/stock invariants, category
/// membership, and any optional supplier intake list, then generates the product's
/// SKU from its category/subcategory names (see `service::sku::generate_sku`) —
/// validation runs first so an invalid category/supplier never consumes a sequence number.
pub async fn create_product(db: &Db, body: CreateProductRequest) -> AppResult<Product> {
    crate::core::logging::domain::tracked("inventory.product_created", async move {
        if body.name.trim().is_empty() {
            return Err(AppError::validation("Product name is required"));
        }
        validate_min_stock_threshold(body.min_stock_threshold)?;
        if let Some(opening) = &body.opening_stock {
            if opening.quantity < 1 {
                return Err(AppError::validation(
                    "Opening stock quantity must be at least 1",
                ));
            }
            super::super::stock_batch::validate_batch_prices(
                opening.unit_cost_cents,
                opening.selling_price_cents,
            )?;
        }
        let (category_name, subcategory_name) =
            ensure_valid_category(db, &body.category_key, &body.subcategory_key).await?;

        // Fail fast: validate suppliers list upfront
        if !body.suppliers.is_empty() {
            let mut seen_keys = std::collections::HashSet::new();
            for intake in &body.suppliers {
                if !seen_keys.insert(&intake.supplier_key) {
                    return Err(AppError::validation(format!(
                        "Duplicate supplierKey '{}' in suppliers list",
                        intake.supplier_key
                    )));
                }
                if intake.quantity < 1 {
                    return Err(AppError::validation(
                        "Supplier intake quantity must be at least 1",
                    ));
                }
                super::super::stock_batch::validate_batch_prices(
                    intake.cost_price_cents,
                    intake.selling_price_cents,
                )?;
                crate::modules::suppliers::service::get_supplier_by_key(db, &intake.supplier_key)
                    .await?;
            }
        }

        let (barcode, barcode_source) =
            resolve_barcode(db, body.barcode, body.auto_generate_barcode).await?;

        let sku = super::super::sku::generate_sku(db, &category_name, &subcategory_name).await?;

        // Defensive fallback only — `generate_sku`'s atomic per-prefix counter
        // already guarantees uniqueness, so this should never actually fire.
        if repository::product::find_product_by_sku(db, &sku)
            .await?
            .is_some()
        {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::SKU_ALREADY_EXISTS,
                format!("A product with SKU '{sku}' already exists"),
            ));
        }

        let now = BsonDateTime::now();
        let document = ProductDocument {
            id: None,
            key: generate_id(prefixes::PRODUCT),
            sku,
            barcode,
            barcode_source,
            name: body.name,
            category_key: body.category_key,
            subcategory_key: body.subcategory_key,
            stock_quantity: 0,
            min_stock_threshold: body.min_stock_threshold,
            is_serialized: body.is_serialized,
            warranty_months: body.warranty_months,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };

        let inserted = repository::product::insert_product(db, document).await?;
        let product_object_id = inserted.id.expect("inserted product must have an id");

        // Units on hand come in as an `opening` batch with their own prices;
        // supplier intakes below each become a purchase batch.
        if let Some(opening) = body.opening_stock {
            super::super::stock_batch::create_batch(
                db,
                super::super::stock_batch::NewBatch {
                    product_id: product_object_id,
                    product_key: inserted.key.clone(),
                    source: BatchSource::Opening,
                    purchase_key: None,
                    supplier_key: None,
                    quantity: opening.quantity,
                    unit_cost_cents: opening.unit_cost_cents,
                    selling_price_cents: opening.selling_price_cents,
                    received_at: now,
                    movement_type: StockMovementType::OpeningBalance,
                    reference_id: Some(inserted.key.clone()),
                    note: Some("Opening balance".to_string()),
                },
            )
            .await?;
        }

        // Process each supplier intake
        for intake in body.suppliers {
            crate::modules::supplier_products::service::link::upsert_link(
                db,
                crate::domain::supplier_products::UpsertSupplierProductLinkRequest {
                    supplier_key: intake.supplier_key.clone(),
                    product_key: inserted.key.clone(),
                    cost_price_cents: Some(intake.cost_price_cents),
                    notes: intake.notes.clone(),
                },
            )
            .await?;

            // Each supplier's intake is its own one-line receipt. A brand-new serialized
            // product's first intake has no way to supply serials here, so it is refused by
            // the receipt's serial check; receive a serialized product's first stock through
            // a receipt after creating it.
            crate::modules::purchases::service::receipt::record_receipt(
                db,
                None,
                crate::domain::purchase_receipts::CreateReceiptRequest {
                    supplier_key: intake.supplier_key,
                    reference_no: intake.reference_no,
                    date: chrono::Utc::now(),
                    notes: intake.notes,
                    lines: vec![crate::domain::purchase_receipts::ReceiptLineRequest {
                        product_key: inserted.key.clone(),
                        quantity: intake.quantity,
                        line_total_cents: intake.quantity * intake.cost_price_cents,
                        selling_price_cents: intake.selling_price_cents,
                        serial_numbers: None,
                    }],
                    payment: None,
                    charges: Vec::new(),
                    charges_payment: None,
                    approval: None,
                    confirm_duplicate: true,
                },
                None,
            )
            .await?;
        }

        let final_product = repository::product::find_product_by_id(db, product_object_id)
            .await?
            .expect("product was just created and must exist");

        super::super::stock_batch::with_price_tiers(
            db,
            final_product.into_product(category_name, subcategory_name),
        )
        .await
    })
    .await
}
