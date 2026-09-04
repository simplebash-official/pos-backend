// Batch import processing for Inventory Products. Validates row fields,
// resolves category and subcategory names to keys, generates sequential
// SKUs and barcodes, and inserts product documents and initial stock movements.

use std::collections::HashSet;

use mongodb::{Database, bson::oid::ObjectId};
use serde_json::Value;

use crate::{
    core::error::AppResult,
    domain::{
        imports::{ImportOptions, ImportRowError},
        inventory::{CategoryInfo, CreateCategoryRequest, CreateProductRequest, StockMovementType},
    },
    modules::inventory::service,
};

/// Result of processing an inventory batch import.
pub struct InventoryImportResult {
    pub successful_count: u64,
    pub failed_count: u64,
    pub errors: Vec<ImportRowError>,
}

fn get_first_string(row: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(val) = row.get(*key) {
            match val {
                Value::String(s) => {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                Value::Number(n) => {
                    return Some(n.to_string());
                }
                _ => {}
            }
        }
    }
    None
}

fn get_money_cents(
    row: &Value,
    cents_keys: &[&str],
    currency_keys: &[&str],
) -> Result<Option<i64>, String> {
    for key in cents_keys {
        if let Some(n) = row.get(*key).and_then(|val| val.as_i64()) {
            return Ok(Some(n));
        }
    }

    for key in currency_keys {
        if let Some(val) = row.get(*key) {
            match val {
                Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        let cents = (f * 100.0).round() as i64;
                        return Ok(Some(cents));
                    }
                }
                Value::String(s) => {
                    let cleaned: String = s
                        .chars()
                        .filter(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    if !cleaned.is_empty() {
                        match cleaned.parse::<f64>() {
                            Ok(f) => {
                                let cents = (f * 100.0).round() as i64;
                                return Ok(Some(cents));
                            }
                            Err(_) => return Err(format!("Invalid price format '{s}'")),
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(None)
}

fn get_i64(row: &Value, keys: &[&str], default: i64) -> Result<i64, String> {
    for key in keys {
        if let Some(val) = row.get(*key) {
            match val {
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        return Ok(i);
                    }
                    if let Some(f) = n.as_f64() {
                        return Ok(f.round() as i64);
                    }
                }
                Value::String(s) => {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        match trimmed.parse::<i64>() {
                            Ok(i) => return Ok(i),
                            Err(_) => {
                                return Err(format!(
                                    "Expected an integer for '{key}', found '{s}'"
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(default)
}

fn get_bool(row: &Value, keys: &[&str], default: bool) -> bool {
    for key in keys {
        if let Some(val) = row.get(*key) {
            match val {
                Value::Bool(b) => return *b,
                Value::String(s) => {
                    let lower = s.trim().to_lowercase();
                    if lower == "true" || lower == "yes" || lower == "1" {
                        return true;
                    }
                    if lower == "false" || lower == "no" || lower == "0" {
                        return false;
                    }
                }
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        return i != 0;
                    }
                }
                _ => {}
            }
        }
    }
    default
}

pub(crate) async fn process_inventory_import(
    db: &Database,
    rows: &[Value],
    options: &ImportOptions,
    file_name: &str,
    batch_key: &str,
) -> AppResult<InventoryImportResult> {
    let mut categories_cache: Vec<CategoryInfo> =
        service::category::list_categories(db).await?.categories;
    let mut seen_barcodes: HashSet<String> = HashSet::new();

    let mut successful_count: u64 = 0;
    let mut failed_count: u64 = 0;
    let mut errors: Vec<ImportRowError> = Vec::new();

    for (idx, row) in rows.iter().enumerate() {
        let row_number = (idx + 1) as u64;

        // 1. Name
        let name = match get_first_string(
            row,
            &[
                "name",
                "productName",
                "product_name",
                "itemName",
                "item_name",
                "title",
            ],
        ) {
            Some(n) if !n.is_empty() => n,
            _ => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("name".to_string()),
                    message: "Product name is required and cannot be blank".to_string(),
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // 2. Category & Subcategory Resolution
        let cat_input = match get_first_string(
            row,
            &[
                "category",
                "categoryName",
                "category_name",
                "categoryKey",
                "category_key",
            ],
        ) {
            Some(c) if !c.is_empty() => c,
            _ => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("category".to_string()),
                    message: "Category is required".to_string(),
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        let subcat_input = match get_first_string(
            row,
            &[
                "subcategory",
                "subCategory",
                "sub_category",
                "subcategoryName",
                "subcategory_name",
                "subcategoryKey",
                "subcategory_key",
            ],
        ) {
            Some(sc) if !sc.is_empty() => sc,
            _ => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("subcategory".to_string()),
                    message: "Subcategory is required".to_string(),
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // Match category
        let mut matched_cat = categories_cache
            .iter()
            .find(|cat| cat.key == cat_input || cat.name.eq_ignore_ascii_case(&cat_input))
            .cloned();

        if matched_cat.is_none() && options.auto_create_categories {
            let create_cat_req = CreateCategoryRequest {
                name: cat_input.clone(),
                icon: "package".to_string(),
                color: "blue".to_string(),
                subcategories: vec![subcat_input.clone()],
            };
            match service::category::create_category(db, create_cat_req).await {
                Ok(created_cat) => {
                    categories_cache.push(created_cat.clone());
                    matched_cat = Some(created_cat);
                }
                Err(e) => {
                    errors.push(ImportRowError {
                        row_number,
                        field: Some("category".to_string()),
                        message: format!("Failed to create category '{cat_input}': {e}"),
                        raw_value: Some(cat_input),
                    });
                    failed_count += 1;
                    continue;
                }
            }
        }

        let cat_info = match matched_cat {
            Some(c) => c,
            None => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("category".to_string()),
                    message: format!("Category '{cat_input}' does not exist in the catalog"),
                    raw_value: Some(cat_input),
                });
                failed_count += 1;
                continue;
            }
        };

        // Match subcategory
        let mut matched_subcat = cat_info
            .subcategories
            .iter()
            .find(|sc| sc.key == subcat_input || sc.name.eq_ignore_ascii_case(&subcat_input))
            .cloned();

        if matched_subcat.is_none() && options.auto_create_categories {
            match service::category::add_subcategory(db, cat_info.key.clone(), subcat_input.clone())
                .await
            {
                Ok(updated_cat) => {
                    let created_subcat = updated_cat
                        .subcategories
                        .iter()
                        .find(|sc| sc.name.eq_ignore_ascii_case(&subcat_input))
                        .cloned();
                    if let Some(pos) = categories_cache
                        .iter_mut()
                        .position(|c| c.key == cat_info.key)
                    {
                        categories_cache[pos] = updated_cat;
                    }
                    matched_subcat = created_subcat;
                }
                Err(e) => {
                    errors.push(ImportRowError {
                        row_number,
                        field: Some("subcategory".to_string()),
                        message: format!("Failed to create subcategory '{subcat_input}': {e}"),
                        raw_value: Some(subcat_input),
                    });
                    failed_count += 1;
                    continue;
                }
            }
        }

        let subcat_info = match matched_subcat {
            Some(sc) => sc,
            None => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("subcategory".to_string()),
                    message: format!(
                        "Subcategory '{subcat_input}' does not exist under category '{}'",
                        cat_info.name
                    ),
                    raw_value: Some(subcat_input),
                });
                failed_count += 1;
                continue;
            }
        };

        // 3. Selling Price
        let selling_price_cents = match get_money_cents(
            row,
            &["sellingPriceCents", "selling_price_cents"],
            &[
                "sellingPrice",
                "selling_price",
                "price",
                "retailPrice",
                "retail_price",
            ],
        ) {
            Ok(Some(cents)) if cents > 0 => cents,
            Ok(Some(cents)) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("sellingPrice".to_string()),
                    message: "Selling price must be greater than 0".to_string(),
                    raw_value: Some(cents.to_string()),
                });
                failed_count += 1;
                continue;
            }
            Ok(None) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("sellingPrice".to_string()),
                    message: "Selling price is required".to_string(),
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
            Err(msg) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("sellingPrice".to_string()),
                    message: msg,
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // 4. Cost Price
        let cost_price_cents = match get_money_cents(
            row,
            &["costPriceCents", "cost_price_cents"],
            &[
                "costPrice",
                "cost_price",
                "cost",
                "wholesalePrice",
                "wholesale_price",
                "buyPrice",
                "buy_price",
            ],
        ) {
            Ok(Some(cents)) if cents >= 0 => cents,
            Ok(Some(cents)) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("costPrice".to_string()),
                    message: "Cost price cannot be negative".to_string(),
                    raw_value: Some(cents.to_string()),
                });
                failed_count += 1;
                continue;
            }
            Ok(None) => 0,
            Err(msg) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("costPrice".to_string()),
                    message: msg,
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // 5. Stock Quantity & Threshold
        let stock_quantity = match get_i64(
            row,
            &[
                "stockQuantity",
                "stock_quantity",
                "stock",
                "quantity",
                "qty",
                "initialStock",
                "initial_stock",
            ],
            0,
        ) {
            Ok(qty) if qty >= 0 => qty,
            Ok(qty) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("stockQuantity".to_string()),
                    message: "Stock quantity cannot be negative".to_string(),
                    raw_value: Some(qty.to_string()),
                });
                failed_count += 1;
                continue;
            }
            Err(msg) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("stockQuantity".to_string()),
                    message: msg,
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        let min_stock_threshold = match get_i64(
            row,
            &[
                "minStockThreshold",
                "min_stock_threshold",
                "minStock",
                "min_stock",
                "alertThreshold",
                "alert_threshold",
            ],
            0,
        ) {
            Ok(th) if th >= 0 => th,
            Ok(th) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("minStockThreshold".to_string()),
                    message: "Minimum stock threshold cannot be negative".to_string(),
                    raw_value: Some(th.to_string()),
                });
                failed_count += 1;
                continue;
            }
            Err(msg) => {
                errors.push(ImportRowError {
                    row_number,
                    field: Some("minStockThreshold".to_string()),
                    message: msg,
                    raw_value: None,
                });
                failed_count += 1;
                continue;
            }
        };

        // 6. Barcode
        let raw_barcode = get_first_string(
            row,
            &["barcode", "barcodeNumber", "barcode_number", "ean", "upc"],
        );
        let (barcode_to_use, auto_gen_barcode) = match raw_barcode {
            Some(bc) => {
                let trimmed = bc.trim();
                if trimmed.is_empty() {
                    (None, options.auto_generate_barcodes)
                } else if !trimmed.chars().all(|c| c.is_ascii_digit())
                    || !(8..=14).contains(&trimmed.len())
                {
                    errors.push(ImportRowError {
                        row_number,
                        field: Some("barcode".to_string()),
                        message: "Barcode must be 8-14 numeric digits".to_string(),
                        raw_value: Some(trimmed.to_string()),
                    });
                    failed_count += 1;
                    continue;
                } else if !seen_barcodes.insert(trimmed.to_string()) {
                    errors.push(ImportRowError {
                        row_number,
                        field: Some("barcode".to_string()),
                        message: format!(
                            "Duplicate barcode '{trimmed}' appears multiple times in file"
                        ),
                        raw_value: Some(trimmed.to_string()),
                    });
                    failed_count += 1;
                    continue;
                } else {
                    // Check against DB via service::product::get_product_by_barcode
                    match service::product::get_product_by_barcode(db, trimmed).await {
                        Ok(_) => {
                            errors.push(ImportRowError {
                                row_number,
                                field: Some("barcode".to_string()),
                                message: format!(
                                    "A product with barcode '{trimmed}' already exists in the catalog"
                                ),
                                raw_value: Some(trimmed.to_string()),
                            });
                            failed_count += 1;
                            continue;
                        }
                        Err(crate::core::error::AppError::NotFound { .. }) => {
                            (Some(trimmed.to_string()), false)
                        }
                        Err(e) => {
                            errors.push(ImportRowError {
                                row_number,
                                field: Some("barcode".to_string()),
                                message: format!("Database lookup failed for barcode: {e}"),
                                raw_value: Some(trimmed.to_string()),
                            });
                            failed_count += 1;
                            continue;
                        }
                    }
                }
            }
            None => (None, options.auto_generate_barcodes),
        };

        // 7. Optional serial & warranty
        let is_serialized = get_bool(
            row,
            &[
                "isSerialized",
                "is_serialized",
                "serialized",
                "trackSerials",
                "track_serials",
            ],
            false,
        );
        let warranty_months =
            match get_i64(row, &["warrantyMonths", "warranty_months", "warranty"], -1) {
                Ok(w) if w >= 0 => Some(w),
                _ => None,
            };

        // Build CreateProductRequest with stock_quantity = 0 initially so apply_stock_delta
        // can apply the delta and record the StockMovement in one atomic operation.
        let create_req = CreateProductRequest {
            barcode: barcode_to_use,
            auto_generate_barcode: auto_gen_barcode,
            name,
            category_key: cat_info.key.clone(),
            subcategory_key: subcat_info.key.clone(),
            cost_price_cents,
            selling_price_cents,
            stock_quantity: 0,
            min_stock_threshold,
            suppliers: Vec::new(),
            is_serialized,
            warranty_months,
        };

        match service::product::create_product(db, create_req).await {
            Ok(product) => {
                // If initial stock quantity was positive, apply delta and record StockMovement
                if let (true, Ok(oid)) = (stock_quantity > 0, ObjectId::parse_str(&product.id)) {
                    let _ = service::stock::apply_stock_delta(
                        db,
                        oid,
                        stock_quantity,
                        StockMovementType::ManualAdjustment,
                        Some(batch_key.to_string()),
                        Some(format!("Initial batch import from {file_name}")),
                    )
                    .await;
                }
                successful_count += 1;
            }
            Err(e) => {
                errors.push(ImportRowError {
                    row_number,
                    field: None,
                    message: e.to_string(),
                    raw_value: None,
                });
                failed_count += 1;
            }
        }
    }

    Ok(InventoryImportResult {
        successful_count,
        failed_count,
        errors,
    })
}
