//! Barcodes tests.

use axum::http::StatusCode;

use serde_json::json;

use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{
    seed_category_with_subcategory, send, send_authed, token_for_role, unique_manual_barcode,
};
use mongodb::bson::DateTime as BsonDateTime;
use uuid::Uuid;

async fn seed_supplier(db: &mongodb::Database) -> String {
    let supplier_key = generate_id("sup");
    let now = BsonDateTime::now();
    db.collection::<mongodb::bson::Document>("suppliers")
        .insert_one(mongodb::bson::doc! {
            "key": &supplier_key,
            "name": format!("Test Supplier {}", Uuid::new_v4()),
            "contact_person": "Jane Doe",
            "primary_phone": "1234567890",
            "address": "123 Main St",
            "supplied_categories": vec!["Widgets"],
            "created_at": now,
            "updated_at": now,
        })
        .await
        .expect("failed to seed supplier");
    supplier_key
}

#[tokio::test]
async fn create_product_with_multiple_suppliers_links_and_records_intake_atomically() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Screens").await;
    let supplier_a_key = seed_supplier(app.mongo()).await;
    let supplier_b_key = seed_supplier(app.mongo()).await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "iPhone 15 Pro Screen (Multi-Supplier)",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "minStockThreshold": 5,
            "suppliers": [
                {
                    "supplierKey": supplier_a_key,
                    "quantity": 30,
                    "costPriceCents": 450,
                    "sellingPriceCents": 1500,
                    "referenceNo": "INV-SUP-A-001",
                    "notes": "Batch A OEM"
                },
                {
                    "supplierKey": supplier_b_key,
                    "quantity": 20,
                    "costPriceCents": 480,
                    "sellingPriceCents": 1500,
                    "referenceNo": "INV-SUP-B-002",
                    "notes": "Batch B Premium"
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["stockQuantity"], 50);
    let (_, as_admin) = send_authed(
        &app.router,
        "GET",
        &format!(
            "/api/inventory/products/{}",
            created["data"]["id"]
                .as_str()
                .expect("the value is a string")
        ),
        None,
        &token_for_role(&app.config, Role::Admin),
    )
    .await;
    // Both intakes sell at Rs.15: one price tier spanning both deliveries' costs.
    let tier = &as_admin["data"]["priceTiers"][0];
    assert_eq!(tier["sellingPriceCents"], 1500);
    assert_eq!(tier["quantity"], 50);
    assert_eq!(tier["minUnitCostCents"], 450);
    assert_eq!(tier["maxUnitCostCents"], 480);

    let product_key = created["data"]["key"]
        .as_str()
        .expect("the value is a string");
    let product_id = created["data"]["id"]
        .as_str()
        .expect("the value is a string");

    // 1. Verify supplier_products links for both suppliers
    let link_a = app
        .mongo()
        .collection::<mongodb::bson::Document>("supplier_products")
        .find_one(mongodb::bson::doc! {
            "supplier_key": &supplier_a_key,
            "product_key": product_key,
        })
        .await
        .expect("query link A")
        .expect("link A must exist");
    assert_eq!(
        link_a
            .get_i64("cost_price_cents")
            .expect("the document has this field"),
        450
    );

    let link_b = app
        .mongo()
        .collection::<mongodb::bson::Document>("supplier_products")
        .find_one(mongodb::bson::doc! {
            "supplier_key": &supplier_b_key,
            "product_key": product_key,
        })
        .await
        .expect("query link B")
        .expect("link B must exist");
    assert_eq!(
        link_b
            .get_i64("cost_price_cents")
            .expect("the document has this field"),
        480
    );

    // 2. Verify purchases collection has 2 records
    let purchases_count = app
        .mongo()
        .collection::<mongodb::bson::Document>("purchases")
        .count_documents(mongodb::bson::doc! { "product_key": product_key })
        .await
        .expect("count purchases");
    assert_eq!(purchases_count, 2);

    // 3. Verify stock movements for the product
    let (status, movements) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{product_id}/movements"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = movements["data"]["movements"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(items.len(), 2);
    let total_delta: i64 = items
        .iter()
        .map(|m| {
            m["quantityDelta"]
                .as_i64()
                .expect("the value is an integer")
        })
        .sum();
    assert_eq!(total_delta, 50);
}

#[tokio::test]
async fn create_product_rejects_duplicate_suppliers_in_same_request() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let supplier_key = seed_supplier(app.mongo()).await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Duplicate Supplier Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "suppliers": [
                {
                    "supplierKey": supplier_key,
                    "quantity": 10,
                    "costPriceCents": 1000,
                    "sellingPriceCents": 2000
                },
                {
                    "supplierKey": supplier_key,
                    "quantity": 5,
                    "costPriceCents": 1100,
                    "sellingPriceCents": 2000
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn create_product_rejects_nonexistent_supplier_in_intake_list() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let valid_supplier_key = seed_supplier(app.mongo()).await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Nonexistent Supplier Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "suppliers": [
                {
                    "supplierKey": valid_supplier_key,
                    "quantity": 10,
                    "costPriceCents": 1000,
                    "sellingPriceCents": 2000
                },
                {
                    "supplierKey": "sup_nonexistent_999",
                    "quantity": 5,
                    "costPriceCents": 1100,
                    "sellingPriceCents": 2000
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "SUPPLIER_NOT_FOUND");
}

#[tokio::test]
async fn create_product_rejects_zero_or_negative_supplier_quantity() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let supplier_key = seed_supplier(app.mongo()).await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Zero Qty Supplier Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "suppliers": [
                {
                    "supplierKey": supplier_key,
                    "quantity": 0,
                    "costPriceCents": 1000,
                    "sellingPriceCents": 2000
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn update_product_can_set_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    // Created with an auto-generated barcode; the shop later wants the real
    // manufacturer number on the package instead.
    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Rebarcoded Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "autoGenerateBarcode": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(created["data"]["barcodeSource"], "generated");

    let manufacturer_barcode = unique_manual_barcode();
    let (status, updated) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "barcode": manufacturer_barcode })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["barcode"], manufacturer_barcode);
    assert_eq!(updated["data"]["barcodeSource"], "manual");

    // And it is now findable by that barcode.
    let (status, listed) = send(
        &app,
        "GET",
        &format!("/api/inventory/products?search={manufacturer_barcode}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = listed["data"]["items"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], id);
}

#[tokio::test]
async fn update_product_barcode_collision_returns_409() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let taken = unique_manual_barcode();

    for name in ["Owner Widget", "Other Widget"] {
        let (status, _) = send(
            &app,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": name,
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
                "minStockThreshold": 1,
                "barcode": if name == "Owner Widget" { taken.clone() } else { unique_manual_barcode() },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let (_, other) = send(
        &app,
        "GET",
        "/api/inventory/products?search=Other%20Widget",
        None,
    )
    .await;
    let other_id = other["data"]["items"][0]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, body) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{other_id}"),
        Some(json!({ "barcode": taken })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "BARCODE_ALREADY_EXISTS");
}

#[tokio::test]
async fn update_product_re_saving_own_barcode_is_not_a_collision() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Self Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, updated) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Self Widget Renamed", "barcode": barcode })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["barcode"], barcode);
}

#[tokio::test]
async fn update_product_rejects_invalid_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Validated Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    for bad in ["abc12345", "1234", "1234567890123456", ""] {
        let (status, body) = send(
            &app,
            "PUT",
            &format!("/api/inventory/products/{id}"),
            Some(json!({ "barcode": bad })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "barcode: {bad:?}");
        assert_eq!(body["code"], "VALIDATION_ERROR");
    }
}

#[tokio::test]
async fn get_product_by_barcode_returns_the_matching_product() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Scannable Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, found) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/by-barcode/{barcode}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found["data"]["id"], id);
    assert_eq!(found["data"]["name"], "Scannable Widget");
    // display names are resolved from the category/subcategory keys, not echoed keys
    assert!(found["data"]["category"].as_str().is_some());
    assert!(found["data"]["subcategory"].as_str().is_some());

    let (status, body) = send(
        &app,
        "GET",
        "/api/inventory/products/by-barcode/99999999999",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "PRODUCT_NOT_FOUND");
}

#[tokio::test]
async fn list_products_filters_by_category_and_search() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Findable Gadget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 5, "unitCostCents": 500, "sellingPriceCents": 1200 },
            "minStockThreshold": 1,
        })),
    )
    .await;

    send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Unrelated Item",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 5, "unitCostCents": 500, "sellingPriceCents": 1200 },
            "minStockThreshold": 1,
        })),
    )
    .await;

    let (status, listed) = send(
        &app,
        "GET",
        &format!("/api/inventory/products?categoryKey={category_key}&search=Findable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = listed["data"]["items"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Findable Gadget");
    assert_eq!(listed["data"]["pagination"]["total"], 1);
}

#[tokio::test]
async fn list_products_search_matches_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, _) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Barcoded Gadget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 5, "unitCostCents": 500, "sellingPriceCents": 1200 },
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, listed) = send(
        &app,
        "GET",
        &format!("/api/inventory/products?search={barcode}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = listed["data"]["items"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Barcoded Gadget");
    assert_eq!(items[0]["barcode"], barcode);
}
