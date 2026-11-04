//! Products tests.

use axum::http::StatusCode;

use serde_json::{Value, json};

use simplebash_pos_backend::domain::users::Role;

use uuid::Uuid;

use crate::common;
use crate::helpers::{
    seed_category_with_subcategory, send, send_authed, token_for_role, unique_manual_barcode,
};

/// Creates a product named `name` and returns its id.
async fn create_named_product(
    app: &common::TestApp,
    category_key: &str,
    subcategory_key: &str,
    name: &str,
    selling_price_cents: i64,
) -> String {
    let (status, created) = send(
        app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": name,
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 5, "unitCostCents": 100, "sellingPriceCents": selling_price_cents },
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string()
}

fn listed_names(body: &Value) -> Vec<String> {
    body["data"]["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|p| {
            p["name"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect()
}

#[tokio::test]
async fn product_list_defaults_to_stable_case_insensitive_name_order() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Sorting").await;
    let tag = Uuid::new_v4().simple().to_string();
    let banana = create_named_product(
        &app,
        &category_key,
        &subcategory_key,
        &format!("{tag} banana"),
        300,
    )
    .await;
    create_named_product(
        &app,
        &category_key,
        &subcategory_key,
        &format!("{tag} Apple"),
        100,
    )
    .await;
    create_named_product(
        &app,
        &category_key,
        &subcategory_key,
        &format!("{tag} cherry"),
        200,
    )
    .await;

    let uri = format!("/api/inventory/products?search={tag}&limit=50");
    let expected = vec![
        format!("{tag} Apple"),
        format!("{tag} banana"),
        format!("{tag} cherry"),
    ];
    let (status, body) = send(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(listed_names(&body), expected);

    // Touching a product bumps `updated_at`; the old default order would have
    // moved it to the front. The list must not change.
    let (status, _) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{banana}"),
        Some(json!({ "sellingPriceCents": 350 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send(&app, "GET", &uri, None).await;
    assert_eq!(listed_names(&body), expected);
}

#[tokio::test]
async fn product_list_honours_explicit_sort_and_breaks_ties_by_name() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Sorting").await;
    let tag = Uuid::new_v4().simple().to_string();
    create_named_product(
        &app,
        &category_key,
        &subcategory_key,
        &format!("{tag} b"),
        200,
    )
    .await;
    create_named_product(
        &app,
        &category_key,
        &subcategory_key,
        &format!("{tag} a"),
        200,
    )
    .await;
    create_named_product(
        &app,
        &category_key,
        &subcategory_key,
        &format!("{tag} c"),
        100,
    )
    .await;

    let base = format!("/api/inventory/products?search={tag}&limit=50&sortBy=sellingPriceCents");
    let (_, asc) = send(&app, "GET", &format!("{base}&sortOrder=asc"), None).await;
    assert_eq!(
        listed_names(&asc),
        vec![format!("{tag} c"), format!("{tag} a"), format!("{tag} b")]
    );
    let (_, desc) = send(&app, "GET", &format!("{base}&sortOrder=desc"), None).await;
    assert_eq!(
        listed_names(&desc),
        vec![format!("{tag} a"), format!("{tag} b"), format!("{tag} c")]
    );

    let (_, name_desc) = send(
        &app,
        "GET",
        &format!("/api/inventory/products?search={tag}&limit=50&sortBy=name&sortOrder=desc"),
        None,
    )
    .await;
    assert_eq!(
        listed_names(&name_desc),
        vec![format!("{tag} c"), format!("{tag} b"), format!("{tag} a")]
    );
}

/// What the shop paid for a product is Admin-only: every product-returning
/// route omits the price tiers' cost range for Manager and Staff, and a write
/// by a Manager still stores the cost it was given.
#[tokio::test]
async fn cost_price_is_returned_to_admin_only() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let barcode = unique_manual_barcode();
    let admin = token_for_role(&app.config, Role::Admin);

    let create_body = json!({
        "name": "Costed Widget",
        "categoryKey": category_key,
        "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
        "barcode": barcode,
        "openingStock": { "quantity": 4, "unitCostCents": 1234, "sellingPriceCents": 2000 },
        "minStockThreshold": 1,
    });
    // Created by a Manager: the response hides cost, the stored value is kept.
    let (status, created) = send(&app, "POST", "/api/inventory/products", Some(create_body)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert!(
        created["data"]["priceTiers"][0]
            .get("minUnitCostCents")
            .is_none()
    );
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let by_id = format!("/api/inventory/products/{id}");
    let by_barcode = format!("/api/inventory/products/by-barcode/{barcode}");
    let list = format!("/api/inventory/products?search={barcode}");

    for role in [Role::Manager, Role::Staff] {
        let token = token_for_role(&app.config, role);
        for uri in [by_id.as_str(), by_barcode.as_str()] {
            let (status, body) = send_authed(&app.router, "GET", uri, None, &token).await;
            assert_eq!(status, StatusCode::OK, "{role:?} {uri}: {body}");
            let tier = &body["data"]["priceTiers"][0];
            assert!(tier.get("minUnitCostCents").is_none(), "{role:?} {uri}");
            assert!(tier.get("maxUnitCostCents").is_none(), "{role:?} {uri}");
            assert_eq!(tier["sellingPriceCents"], 2000);
        }
        let (status, body) = send_authed(&app.router, "GET", &list, None, &token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let items = body["data"]["items"]
            .as_array()
            .expect("the value is an array");
        assert_eq!(items.len(), 1);
        assert!(
            items[0]["priceTiers"][0].get("minUnitCostCents").is_none(),
            "{role:?} list"
        );
    }

    for uri in [by_id.as_str(), by_barcode.as_str()] {
        let (status, body) = send_authed(&app.router, "GET", uri, None, &admin).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["data"]["priceTiers"][0]["minUnitCostCents"], 1234,
            "{uri}"
        );
    }
    let (status, body) = send_authed(&app.router, "GET", &list, None, &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["items"][0]["priceTiers"][0]["maxUnitCostCents"],
        1234
    );

    // A Manager re-pricing the batch leaves its stored cost alone.
    let (_, batches) = send(
        &app,
        "GET",
        &format!(
            "/api/inventory/products/{}/batches",
            created["data"]["key"]
                .as_str()
                .expect("the value is a string")
        ),
        None,
    )
    .await;
    let batch_key = batches["data"]["items"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert!(batches["data"]["items"][0].get("unitCostCents").is_none());
    let (status, updated) = send(
        &app,
        "PATCH",
        &format!("/api/inventory/batches/{batch_key}"),
        Some(json!({ "sellingPriceCents": 2500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert!(updated["data"].get("unitCostCents").is_none());
    let (_, body) = send_authed(&app.router, "GET", &by_id, None, &admin).await;
    assert_eq!(body["data"]["priceTiers"][0]["minUnitCostCents"], 1234);
    assert_eq!(body["data"]["priceTiers"][0]["sellingPriceCents"], 2500);
}

#[tokio::test]
async fn product_lifecycle_create_get_update_stock_and_delete() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Test Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 10, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 3,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["success"], true);
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(created["data"]["categoryKey"], category_key);
    assert_eq!(created["data"]["subcategoryKey"], subcategory_key);
    // `category` is seeded as "Test Category <uuid>" -> derived code "TES";
    // "Widgets" -> "WID" (see service::sku::derive_code).
    let sku = created["data"]["sku"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert!(sku.starts_with("TES-WID-"), "unexpected sku: {sku}");
    assert_eq!(created["data"]["stockQuantity"], 10);

    let (status, fetched) = send(&app, "GET", &format!("/api/inventory/products/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["data"]["id"], id);

    let (status, updated) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Updated Widget" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], "Updated Widget");
    assert_eq!(updated["data"]["priceTiers"][0]["sellingPriceCents"], 2000);
    assert_eq!(updated["data"]["sku"], sku);

    // Taking stock out names the batch it comes from.
    let (_, batches) = send(
        &app,
        "GET",
        &format!(
            "/api/inventory/products/{}/batches",
            created["data"]["key"]
                .as_str()
                .expect("the value is a string")
        ),
        None,
    )
    .await;
    let batch_key = batches["data"]["items"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (status, missing_batch) = send(
        &app,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -4, "reason": "test sale" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{missing_batch}");
    let (status, adjusted) = send(
        &app,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -4, "reason": "test sale", "batchKey": batch_key })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(adjusted["data"]["stockQuantity"], 6);
    assert_eq!(adjusted["data"]["previousStockQuantity"], 10);
    assert_eq!(adjusted["data"]["delta"], -4);

    let (status, insufficient) = send(
        &app,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -100, "batchKey": batch_key })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(insufficient["code"], "INSUFFICIENT_STOCK");
    assert_eq!(insufficient["details"]["available"], 6);
    assert_eq!(insufficient["details"]["requested"], 100);

    let (status, movements) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{id}/movements"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = movements["data"]["movements"]
        .as_array()
        .expect("the value is an array");
    // Oldest first: the opening balance that creating the product with
    // `openingStock.quantity: 10` recorded, then the manual adjustment —
    // both against the same batch.
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[0]["quantityDelta"], 10);
    assert_eq!(list[0]["type"], "opening_balance");
    assert_eq!(list[0]["batchKey"], batch_key);
    assert_eq!(list[1]["batchKey"], batch_key);
    assert_eq!(list[1]["quantityDelta"], -4);
    assert_eq!(list[1]["type"], "manual_adjustment");

    let (status, deleted) = send(
        &app,
        "DELETE",
        &format!("/api/inventory/products/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["data"]["id"], id);

    let (status, missing) = send(&app, "GET", &format!("/api/inventory/products/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "PRODUCT_NOT_FOUND");
}

#[tokio::test]
async fn create_product_validates_category_and_pricing_and_auto_generates_sequential_skus() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let (status, invalid_category) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Bad Category Widget",
            "categoryKey": category_key,
            "subcategoryKey": "subcat_does_not_exist", "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_category["code"], "VALIDATION_ERROR");

    // A subcategory that exists but belongs to a *different* category must
    // also be rejected — proves the category_key/subcategory_key pairing is
    // actually enforced, not just "does this subcategory key exist anywhere".
    let (other_category_key, other_subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Gadgets").await;
    let (status, mismatched_category) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Mismatched Widget",
            "categoryKey": category_key,
            "subcategoryKey": other_subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(mismatched_category["code"], "VALIDATION_ERROR");
    let _ = other_category_key;

    let (status, _bad_price) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Free Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 0 },
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Two products created back-to-back under the same category/subcategory
    // should get SKUs sharing a prefix but with sequential, distinct suffixes
    // — proving the atomic per-prefix counter, not client input, drives them.
    let body = json!({
        "name": "Sequential Widget",
        "categoryKey": category_key,
        "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
        "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
        "minStockThreshold": 1,
    });
    let (status, first) = send(&app, "POST", "/api/inventory/products", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    let first_sku = first["data"]["sku"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, second) = send(&app, "POST", "/api/inventory/products", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let second_sku = second["data"]["sku"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    assert_ne!(first_sku, second_sku);
    let prefix = first_sku.rsplit_once('-').expect("the step succeeds").0;
    assert!(second_sku.starts_with(&format!("{prefix}-")));
}

#[tokio::test]
async fn create_product_auto_generates_sequential_ean13_barcodes() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let body = json!({
        "name": "Auto Barcode Widget",
        "categoryKey": category_key,
        "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
        "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
        "minStockThreshold": 1,
        "autoGenerateBarcode": true,
    });

    let (status, first) = send(&app, "POST", "/api/inventory/products", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    let first_barcode = first["data"]["barcode"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(first_barcode.len(), 13);
    assert!(first_barcode.chars().all(|c| c.is_ascii_digit()));
    assert!(first_barcode.starts_with("20"));
    assert_eq!(first["data"]["barcodeSource"], "generated");

    let (status, second) = send(&app, "POST", "/api/inventory/products", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let second_barcode = second["data"]["barcode"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_ne!(first_barcode, second_barcode);

    let first_seq: i64 = first_barcode[2..12].parse().expect("the text parses");
    let second_seq: i64 = second_barcode[2..12].parse().expect("the text parses");
    assert_eq!(second_seq, first_seq + 1);
}

#[tokio::test]
async fn create_product_accepts_manual_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Manual Barcode Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["barcode"], barcode);
    assert_eq!(created["data"]["barcodeSource"], "manual");
}

#[tokio::test]
async fn create_product_rejects_invalid_manual_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    for bad_barcode in ["abc12345", "1234", "1234567890123456"] {
        let (status, body) = send(
            &app,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Invalid Barcode Widget",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
                "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
                "minStockThreshold": 1,
                "barcode": bad_barcode,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "barcode: {bad_barcode}");
        assert_eq!(body["code"], "VALIDATION_ERROR");
    }
}

#[tokio::test]
async fn create_product_rejects_duplicate_manual_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, _first) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "First Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, second) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Second Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(second["code"], "BARCODE_ALREADY_EXISTS");
}

#[tokio::test]
async fn create_product_rejects_barcode_and_auto_generate_together() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Conflicting Barcode Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
            "barcode": "12345678",
            "autoGenerateBarcode": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn create_product_without_barcode_persists_null() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Barcode-Free Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 1, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(created["data"]["barcode"].is_null());
    assert!(created["data"]["barcodeSource"].is_null());
}
