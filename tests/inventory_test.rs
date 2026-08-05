mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jana2u_pos_backend::{
    core::{config::Config, id::generate_id},
    domain::users::Role,
    modules::inventory::model::{CategoryDocument, SubcategoryDocument},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
}

/// Same as `send`, but with an `Authorization: Bearer <token>` header —
/// for the admin-only category management endpoints.
async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
}

/// Mints a JWT with the given role claim (or none) and no permissions —
/// `AdminUser` only checks `role`, never `permissions`.
fn token_with_role(config: &Config, role: Option<Role>) -> String {
    common::mint_token(config, role, &[])
}

fn admin_token(config: &Config) -> String {
    token_with_role(config, Some(Role::Admin))
}

use mongodb::bson::DateTime as BsonDateTime;

/// Inserts a category directly into the test database rather than through
/// the admin API, so tests that don't care about category management
/// itself (e.g. product CRUD) don't need an admin token just to set up
/// fixture data. Uses a random name so parallel tests never collide with
/// each other or with real seeded data. Returns the category's key.
async fn seed_category(db: &mongodb::Database) -> String {
    let name = format!("Test Category {}", Uuid::new_v4());
    let key = generate_id("cat");
    let now = BsonDateTime::now();
    db.collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: key.clone(),
            name,
            icon: "Box".to_string(),
            color: "gray".to_string(),
            created_at: now,
            updated_at: now,
        })
        .await
        .expect("failed to seed category");
    key
}

/// Inserts a subcategory directly into the test database under
/// `category_key`. Returns the subcategory's key.
async fn seed_subcategory(db: &mongodb::Database, category_key: &str, name: &str) -> String {
    let key = generate_id("subcat");
    let now = BsonDateTime::now();
    db.collection::<SubcategoryDocument>("subcategories")
        .insert_one(SubcategoryDocument {
            id: None,
            key: key.clone(),
            category_key: category_key.to_string(),
            name: name.to_string(),
            created_at: now,
            updated_at: now,
        })
        .await
        .expect("failed to seed subcategory");
    key
}

/// Convenience wrapper: seeds a category plus one subcategory under it,
/// returning `(category_key, subcategory_key)` — the shape most product
/// tests need.
async fn seed_category_with_subcategory(
    db: &mongodb::Database,
    subcategory_name: &str,
) -> (String, String) {
    let category_key = seed_category(db).await;
    let subcategory_key = seed_subcategory(db, &category_key, subcategory_name).await;
    (category_key, subcategory_key)
}

#[tokio::test]
async fn product_lifecycle_create_get_update_stock_and_delete() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (status, created) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Test Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 10,
            "minStockThreshold": 3,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["success"], true);
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["categoryKey"], category_key);
    assert_eq!(created["data"]["subcategoryKey"], subcategory_key);
    // `category` is seeded as "Test Category <uuid>" -> derived code "TES";
    // "Widgets" -> "WID" (see service::sku::derive_code).
    let sku = created["data"]["sku"].as_str().unwrap().to_string();
    assert!(sku.starts_with("TES-WID-"), "unexpected sku: {sku}");
    assert_eq!(created["data"]["stockQuantity"], 10);

    let (status, fetched) = send(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["data"]["id"], id);

    let (status, updated) = send(
        &app.router,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Updated Widget", "sellingPriceCents": 2500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], "Updated Widget");
    assert_eq!(updated["data"]["sellingPriceCents"], 2500);
    assert_eq!(updated["data"]["sku"], sku);

    let (status, adjusted) = send(
        &app.router,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -4, "reason": "test sale" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(adjusted["data"]["stockQuantity"], 6);
    assert_eq!(adjusted["data"]["previousStockQuantity"], 10);
    assert_eq!(adjusted["data"]["delta"], -4);

    let (status, insufficient) = send(
        &app.router,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -100 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(insufficient["code"], "INSUFFICIENT_STOCK");

    let (status, movements) = send(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{id}/movements"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = movements["data"]["movements"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["quantityDelta"], -4);
    assert_eq!(list[0]["type"], "manual_adjustment");

    let (status, deleted) = send(
        &app.router,
        "DELETE",
        &format!("/api/inventory/products/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["data"]["id"], id);

    let (status, missing) = send(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "PRODUCT_NOT_FOUND");
}

#[tokio::test]
async fn create_product_validates_category_and_pricing_and_auto_generates_sequential_skus() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (status, invalid_category) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Bad Category Widget",
            "categoryKey": category_key,
            "subcategoryKey": "subcat_does_not_exist",
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
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
        seed_category_with_subcategory(&app.db, "Gadgets").await;
    let (status, mismatched_category) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Mismatched Widget",
            "categoryKey": category_key,
            "subcategoryKey": other_subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(mismatched_category["code"], "VALIDATION_ERROR");
    let _ = other_category_key;

    let (status, _bad_price) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Free Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 0,
            "stockQuantity": 1,
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
        "subcategoryKey": subcategory_key,
        "costPriceCents": 1000,
        "sellingPriceCents": 2000,
        "stockQuantity": 1,
        "minStockThreshold": 1,
    });
    let (status, first) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_sku = first["data"]["sku"].as_str().unwrap().to_string();

    let (status, second) = send(&app.router, "POST", "/api/inventory/products", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let second_sku = second["data"]["sku"].as_str().unwrap().to_string();

    assert_ne!(first_sku, second_sku);
    let prefix = first_sku.rsplit_once('-').unwrap().0;
    assert!(second_sku.starts_with(&format!("{prefix}-")));
}

#[tokio::test]
async fn list_products_filters_by_category_and_search() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Findable Gadget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1200,
            "stockQuantity": 5,
            "minStockThreshold": 1,
        })),
    )
    .await;

    send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Unrelated Item",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1200,
            "stockQuantity": 5,
            "minStockThreshold": 1,
        })),
    )
    .await;

    let (status, listed) = send(
        &app.router,
        "GET",
        &format!("/api/inventory/products?categoryKey={category_key}&search=Findable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = listed["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Findable Gadget");
    assert_eq!(listed["data"]["pagination"]["total"], 1);
}

#[tokio::test]
async fn delete_products_batch_removes_multiple() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let mut ids = Vec::new();
    for _ in 0..2 {
        let (_, created) = send(
            &app.router,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Batch Widget",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "costPriceCents": 500,
                "sellingPriceCents": 1200,
                "stockQuantity": 5,
                "minStockThreshold": 1,
            })),
        )
        .await;
        ids.push(created["data"]["id"].as_str().unwrap().to_string());
    }

    let (status, result) = send(
        &app.router,
        "DELETE",
        "/api/inventory/products",
        Some(json!({ "productIds": ids })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["data"]["deletedCount"], 2);

    for id in ids {
        let (status, _) = send(
            &app.router,
            "GET",
            &format!("/api/inventory/products/{id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn low_stock_lists_products_at_or_below_threshold() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (_, created) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Low Stock Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1200,
            "stockQuantity": 2,
            "minStockThreshold": 5,
        })),
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let (status, low_stock) = send(
        &app.router,
        "GET",
        "/api/inventory/products/low-stock",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = low_stock["data"]["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["id"] == id));
}

#[tokio::test]
async fn category_endpoints_read_seeded_categories() {
    let app = common::spawn_app().await;
    let category_key = seed_category(&app.db).await;
    seed_subcategory(&app.db, &category_key, "Alpha").await;
    seed_subcategory(&app.db, &category_key, "Beta").await;

    let (status, all) = send(&app.router, "GET", "/api/inventory/categories", None).await;
    assert_eq!(status, StatusCode::OK);
    let categories = all["data"]["categories"].as_array().unwrap();
    assert!(
        categories
            .iter()
            .any(|entry| entry["key"] == category_key
                && entry["subcategories"][0]["name"] == "Alpha")
    );

    let (status, subs) = send(
        &app.router,
        "GET",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let subcategory_names: Vec<&str> = subs["data"]["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(subcategory_names, vec!["Alpha", "Beta"]);

    let (status, missing) = send(
        &app.router,
        "GET",
        "/api/inventory/categories/cat_does_not_exist/subcategories",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "CATEGORY_NOT_FOUND");

    let (status, valid) = send(&app.router, "GET", "/api/inventory/categories/valid", None).await;
    assert_eq!(status, StatusCode::OK);
    let valid_categories = valid["data"]["categories"].as_array().unwrap();
    let entry = valid_categories
        .iter()
        .find(|entry| entry["key"] == category_key)
        .expect("seeded category present in valid categories response");
    let valid_subcategory_names: Vec<&str> = entry["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(valid_subcategory_names, vec!["Alpha", "Beta"]);
}

#[tokio::test]
async fn category_admin_endpoints_require_admin_role() {
    let app = common::spawn_app().await;
    let body = Some(json!({
        "name": format!("Unauthorized Category {}", Uuid::new_v4()),
        "icon": "Box",
        "color": "gray",
    }));

    let (status, no_auth) = send(
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
            "color": "gray",
            "subcategories": ["Alpha"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["name"], name);
    let category_key = created["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["subcategories"][0]["name"], "Alpha");
    let alpha_key = created["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, duplicate) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": name, "icon": "Box", "color": "gray" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["code"], "CATEGORY_ALREADY_EXISTS");

    let (_, product) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Cascade Widget",
            "categoryKey": category_key,
            "subcategoryKey": alpha_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;
    let product_id = product["data"]["id"].as_str().unwrap().to_string();

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
        &app.router,
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
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
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
        &app.router,
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
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
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
        &app.router,
        "GET",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "CATEGORY_NOT_FOUND");
}
