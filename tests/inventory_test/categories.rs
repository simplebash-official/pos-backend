//! Categories tests.

use axum::http::StatusCode;

use serde_json::json;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::codes;

use simplebash_pos_backend::domain::users::Role;

use uuid::Uuid;

use crate::common;
use crate::helpers::{
    READ_ROUTES, manager_token, seed_category, seed_category_with_subcategory, seed_subcategory,
    send, send_anon, send_authed,
};

/// Mints a JWT with the given role claim (or none) and no permissions.
/// Only for the `AdminUser`-gated category/subcategory routes, which check
/// `role` and never `permissions` — a token from here cannot pass the
/// `require_permission` gate on the product/stock/category-read routes.
fn token_with_role(config: &Config, role: Option<Role>) -> String {
    common::mint_token(config, role, &[])
}

fn admin_token(config: &Config) -> String {
    token_with_role(config, Some(Role::Admin))
}

const WRITE_ROUTES: &[(&str, &str)] = &[
    ("POST", "/api/inventory/products"),
    ("DELETE", "/api/inventory/products"),
];

#[tokio::test]
async fn delete_products_batch_removes_multiple() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let mut ids = Vec::new();
    for _ in 0..2 {
        let (_, created) = send(
            &app,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Batch Widget",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "openingStock": { "quantity": 5, "unitCostCents": 500, "sellingPriceCents": 1200 },
                "minStockThreshold": 1,
            })),
        )
        .await;
        ids.push(
            created["data"]["id"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }

    let (status, result) = send(
        &app,
        "DELETE",
        "/api/inventory/products",
        Some(json!({ "productIds": ids })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["data"]["deletedCount"], 2);

    for id in ids {
        let (status, _) = send(&app, "GET", &format!("/api/inventory/products/{id}"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn low_stock_lists_products_at_or_below_threshold() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;

    let (_, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Low Stock Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 2, "unitCostCents": 500, "sellingPriceCents": 1200 },
            "minStockThreshold": 5,
        })),
    )
    .await;
    let id = created["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, low_stock) = send(&app, "GET", "/api/inventory/products/low-stock", None).await;
    assert_eq!(status, StatusCode::OK);
    let items = low_stock["data"]["items"]
        .as_array()
        .expect("the value is an array");
    assert!(items.iter().any(|item| item["id"] == id));
}

#[tokio::test]
async fn category_endpoints_read_seeded_categories() {
    let app = common::spawn_app().await;
    let category_key = seed_category(app.mongo()).await;
    seed_subcategory(app.mongo(), &category_key, "Alpha").await;
    seed_subcategory(app.mongo(), &category_key, "Beta").await;

    let (status, all) = send(&app, "GET", "/api/inventory/categories", None).await;
    assert_eq!(status, StatusCode::OK);
    let categories = all["data"]["categories"]
        .as_array()
        .expect("the value is an array");
    assert!(
        categories
            .iter()
            .any(|entry| entry["key"] == category_key
                && entry["subcategories"][0]["name"] == "Alpha")
    );

    let (status, subs) = send(
        &app,
        "GET",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let subcategory_names: Vec<&str> = subs["data"]["subcategories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|entry| entry["name"].as_str().expect("the value is a string"))
        .collect();
    assert_eq!(subcategory_names, vec!["Alpha", "Beta"]);

    let (status, missing) = send(
        &app,
        "GET",
        "/api/inventory/categories/cat_does_not_exist/subcategories",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "CATEGORY_NOT_FOUND");

    let (status, valid) = send(&app, "GET", "/api/inventory/categories/valid", None).await;
    assert_eq!(status, StatusCode::OK);
    let valid_categories = valid["data"]["categories"]
        .as_array()
        .expect("the value is an array");
    let entry = valid_categories
        .iter()
        .find(|entry| entry["key"] == category_key)
        .expect("seeded category present in valid categories response");
    let valid_subcategory_names: Vec<&str> = entry["subcategories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|s| s["name"].as_str().expect("the value is a string"))
        .collect();
    assert_eq!(valid_subcategory_names, vec!["Alpha", "Beta"]);
}

#[tokio::test]
async fn category_admin_endpoints_require_admin_role() {
    let app = common::spawn_app().await;
    let body = Some(json!({
        "name": format!("Unauthorized Category {}", Uuid::new_v4()),
        "icon": "Box",
        "color": "blue",
    }));

    let (status, no_auth) = send_anon(
        &app.router,
        "POST",
        "/api/inventory/categories",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(no_auth["code"], "UNAUTHORIZED");

    let non_admin = token_with_role(&app.config, Some(Role::Staff));
    let (status, forbidden) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        body,
        &non_admin,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden["code"], "ADMIN_REQUIRED");
}

#[tokio::test]
async fn category_admin_crud_lifecycle_and_in_use_guards() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let name = format!("Admin Category {}", Uuid::new_v4());

    let (status, created) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": name,
            "icon": "Box",
            "color": "blue",
            "subcategories": ["Alpha"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["name"], name);
    let category_key = created["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(created["data"]["subcategories"][0]["name"], "Alpha");
    let alpha_key = created["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, duplicate) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": name, "icon": "Box", "color": "blue" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["code"], "CATEGORY_ALREADY_EXISTS");

    let (_, product) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Cascade Widget",
            "categoryKey": category_key,
            "subcategoryKey": alpha_key,
            "openingStock": { "quantity": 1, "unitCostCents": 500, "sellingPriceCents": 1000 },
            "minStockThreshold": 1,
        })),
    )
    .await;
    let product_id = product["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // Renaming the category is now a pure display-label change — the
    // product's categoryKey (the actual FK) must stay exactly the same.
    let renamed = format!("{name} Renamed");
    let (status, updated) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "name": renamed })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], renamed);
    assert_eq!(updated["data"]["key"], category_key);

    let (_, fetched_product) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
    )
    .await;
    assert_eq!(fetched_product["data"]["categoryKey"], category_key);
    assert_eq!(fetched_product["data"]["category"], renamed);

    // Category still has a product on it, and the subcategory does too.
    let (status, category_in_use) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(category_in_use["code"], "CATEGORY_IN_USE");

    let (status, subcategory_in_use) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}/subcategories/{alpha_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(subcategory_in_use["code"], "SUBCATEGORY_IN_USE");

    // Add a second subcategory, then free up the product so both deletes
    // (subcategory, then category) can succeed.
    let (status, with_beta) = send_authed(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Beta" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let subcategory_names: Vec<&str> = with_beta["data"]["subcategories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|s| s["name"].as_str().expect("the value is a string"))
        .collect();
    assert_eq!(subcategory_names, vec!["Alpha", "Beta"]);

    let (status, duplicate_sub) = send_authed(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Beta" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate_sub["code"], "SUBCATEGORY_ALREADY_EXISTS");

    send(
        &app,
        "DELETE",
        &format!("/api/inventory/products/{product_id}"),
        None,
    )
    .await;

    let (status, subcategory_removed) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}/subcategories/{alpha_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let remaining_names: Vec<&str> = subcategory_removed["data"]["subcategories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|s| s["name"].as_str().expect("the value is a string"))
        .collect();
    assert_eq!(remaining_names, vec!["Beta"]);

    let (status, deleted) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["data"]["name"], renamed);

    let (status, missing) = send(
        &app,
        "GET",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "CATEGORY_NOT_FOUND");
}

#[tokio::test]
async fn category_field_validation() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);

    // 1. POST with empty icon -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Empty Icon Cat {}", Uuid::new_v4()),
            "icon": "   ",
            "color": "blue",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 2. POST with empty color -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Empty Color Cat {}", Uuid::new_v4()),
            "icon": "DeviceLaptop",
            "color": "   ",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 3. POST with valid icon & color -> 201 CREATED
    let (status, created) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Valid Category {}", Uuid::new_v4()),
            "icon": "DeviceLaptop",
            "color": "indigo",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let category_key = created["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // 4. PUT with empty icon -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "icon": "" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 5. PUT with empty color -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "color": "   " })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 6. PUT with valid custom icon & color -> 200 OK
    let (status, updated) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "icon": "BrandGithub", "color": "custom-theme-color" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["icon"], "BrandGithub");
    assert_eq!(updated["data"]["color"], "custom-theme-color");
}

#[tokio::test]
async fn inventory_overview_endpoint_returns_metrics_hierarchical_data_and_filters() {
    let app = common::spawn_app().await;

    // 1. Seed test categories and subcategories
    let (cat1_key, subcat1_key) = seed_category_with_subcategory(app.mongo(), "LED Strips").await;
    let subcat2_key = seed_subcategory(app.mongo(), &cat1_key, "Smart Watches").await;

    // 2. Seed products with different stock levels
    // Normal stock product under LED Strips
    let (status, _p1) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "RGB LED Strip Light",
            "categoryKey": cat1_key,
            "subcategoryKey": subcat1_key,
            "openingStock": { "quantity": 50, "unitCostCents": 1000, "sellingPriceCents": 1800 },
            "minStockThreshold": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Low stock product under LED Strips (stock 5 <= threshold 10)
    let (status, _p2) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Smart RGB Corner Floor Lamp",
            "categoryKey": cat1_key,
            "subcategoryKey": subcat1_key,
            "openingStock": { "quantity": 5, "unitCostCents": 3000, "sellingPriceCents": 6000 },
            "minStockThreshold": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Normal stock product under Smart Watches
    let (status, _p3) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Ultra Smartwatch Screen Protector",
            "categoryKey": cat1_key,
            "subcategoryKey": subcat2_key,
            "openingStock": { "quantity": 20, "unitCostCents": 200, "sellingPriceCents": 500 },
            "minStockThreshold": 5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 3. Test GET /api/inventory/overview without filters
    let (status, overview) = send(&app, "GET", "/api/inventory/overview", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["success"], true);

    let metrics = &overview["data"]["metrics"];
    assert!(
        metrics["totalItems"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 3
    );
    assert!(
        metrics["totalCategories"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 1
    );
    assert!(
        metrics["totalSubcategories"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 2
    );
    assert!(
        metrics["lowStockAlerts"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 1
    );

    let categories = overview["data"]["categories"]
        .as_array()
        .expect("the value is an array");
    let cat1 = categories
        .iter()
        .find(|c| c["key"] == cat1_key)
        .expect("category 1 should be in overview");
    assert!(
        cat1["totalItems"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 3
    );
    assert!(
        cat1["subcategoriesCount"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 2
    );

    let subcats = cat1["subcategories"]
        .as_array()
        .expect("the value is an array");
    let subcat1 = subcats
        .iter()
        .find(|s| s["key"] == subcat1_key)
        .expect("subcat 1 should be present");
    assert_eq!(subcat1["totalItems"], 2);
    let products_list = subcat1["products"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(products_list.len(), 2);
    assert_eq!(subcat1["pagination"]["limit"], 10);

    // 4. Test GET /api/inventory/overview?lowStock=true
    let (status, low_stock_overview) =
        send(&app, "GET", "/api/inventory/overview?lowStock=true", None).await;
    assert_eq!(status, StatusCode::OK);
    let low_stock_cat1 = low_stock_overview["data"]["categories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["key"] == cat1_key)
        .expect("a match exists");
    let low_stock_subcat1 = low_stock_cat1["subcategories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|s| s["key"] == subcat1_key)
        .expect("a match exists");
    assert_eq!(low_stock_subcat1["totalItems"], 1);
    assert_eq!(
        low_stock_subcat1["products"][0]["name"],
        "Smart RGB Corner Floor Lamp"
    );

    // 5. Test GET /api/inventory/overview?search=LED
    let (status, search_overview) =
        send(&app, "GET", "/api/inventory/overview?search=LED", None).await;
    assert_eq!(status, StatusCode::OK);
    let search_cat1 = search_overview["data"]["categories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["key"] == cat1_key)
        .expect("a match exists");
    let search_subcat1 = search_cat1["subcategories"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|s| s["key"] == subcat1_key)
        .expect("a match exists");
    assert_eq!(search_subcat1["totalItems"], 2);
}

#[tokio::test]
async fn inventory_routes_reject_requests_without_a_token() {
    let app = common::spawn_app().await;

    for (method, uri) in READ_ROUTES.iter().chain(WRITE_ROUTES) {
        let body = (*method != "GET").then(|| json!({ "productIds": [] }));
        let (status, json) = send_anon(&app.router, method, uri, body).await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must require a token, got {status}: {json}"
        );
        assert_eq!(json["code"], codes::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn inventory_routes_reject_a_token_signed_with_the_wrong_secret() {
    let app = common::spawn_app().await;

    // Same claims, different signing key — proves the signature is actually
    // verified rather than the payload merely being decoded.
    let mut forged_config = (*app.config).clone();
    forged_config.jwt_secret = format!("not-{}", app.config.jwt_secret);
    let forged = manager_token(&forged_config);

    for (method, uri) in [("GET", "/api/inventory/products"), WRITE_ROUTES[0]] {
        let body = (method != "GET").then(|| json!({}));
        let (status, _) = send_authed(&app.router, method, uri, body, &forged).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    let (status, _) = send_authed(
        &app.router,
        "GET",
        "/api/inventory/products",
        None,
        "not-a-jwt-at-all",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
