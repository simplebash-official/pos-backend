//! Permissions tests.

use axum::http::StatusCode;

use serde_json::{Value, json};
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::codes;
use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::inventory::model::ProductDocument;

use crate::common;
use crate::helpers::{
    READ_ROUTES, seed_category_with_subcategory, seed_subcategory, send, send_anon, send_authed,
    token_for_role,
};
use mongodb::bson::DateTime as BsonDateTime;
use uuid::Uuid;

/// Staff: holds `inventory:read` only — the role that proves reads and
/// writes are gated separately rather than both collapsing to "logged in".
fn staff_token(config: &Config) -> String {
    token_for_role(config, Role::Staff)
}

/// Inserts a minimal `ProductDocument` directly, so the stats test can
/// control `stock_quantity`/`min_stock_threshold`/`deleted_at` precisely.
async fn seed_product_with(
    db: &mongodb::Database,
    category_key: &str,
    subcategory_key: &str,
    stock_quantity: i64,
    min_stock_threshold: i64,
    deleted: bool,
) {
    let key = generate_id("prd");
    let now = BsonDateTime::now();
    db.collection::<ProductDocument>("products")
        .insert_one(ProductDocument {
            id: None,
            key,
            sku: format!("SKU-{}", Uuid::new_v4().simple()),
            barcode: None,
            barcode_source: None,
            name: "Stats Test Product".to_string(),
            category_key: category_key.to_string(),
            subcategory_key: subcategory_key.to_string(),
            stock_quantity,
            min_stock_threshold,
            is_serialized: false,
            warranty_months: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: if deleted { Some(now) } else { None },
            updated_by_device: None,
        })
        .await
        .expect("failed to seed product");
}

#[tokio::test]
async fn staff_may_read_inventory_but_not_write_it() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);

    // Staff holds `inventory:read`, so every read succeeds.
    for (method, uri) in READ_ROUTES {
        let (status, json) = send_authed(&app.router, method, uri, None, &staff).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri} for staff: {json}");
    }

    // ...but not `inventory:write`, so every mutation is refused. This is the
    // distinction that would collapse if the routes were gated on "any
    // authenticated caller" instead of on a permission.
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Alpha").await;
    let (status, product) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": format!("Staff Guard Product {}", Uuid::new_v4()),
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 5, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "fixture product: {product}");
    let product_id = product["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (_, batches) = send(
        &app,
        "GET",
        &format!(
            "/api/inventory/products/{}/batches",
            product["data"]["key"]
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

    let forbidden_writes: Vec<(&str, String, Option<Value>)> = vec![
        (
            "POST",
            "/api/inventory/products".to_string(),
            Some(json!({
                "name": "Nope",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "openingStock": { "quantity": 1, "unitCostCents": 100, "sellingPriceCents": 200 },
                "minStockThreshold": 1,
            })),
        ),
        (
            "PUT",
            format!("/api/inventory/products/{product_id}"),
            Some(json!({ "minStockThreshold": 9 })),
        ),
        (
            "PATCH",
            format!("/api/inventory/batches/{batch_key}"),
            Some(json!({ "sellingPriceCents": 100 })),
        ),
        (
            "DELETE",
            format!("/api/inventory/products/{product_id}"),
            None,
        ),
        (
            "DELETE",
            "/api/inventory/products".to_string(),
            Some(json!({ "productIds": [product_id] })),
        ),
        (
            "PATCH",
            format!("/api/inventory/products/{product_id}/stock"),
            Some(
                json!({ "delta": 5, "reason": "test", "unitCostCents": 1000, "sellingPriceCents": 2000 }),
            ),
        ),
    ];

    for (method, uri, body) in forbidden_writes {
        let (status, json) = send_authed(&app.router, method, &uri, body, &staff).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {uri} for staff: {json}"
        );
        assert_eq!(json["code"], codes::PERMISSION_DENIED, "{method} {uri}");
    }

    // The product is untouched by the six refused writes.
    let (status, still_there) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{still_there}");
    assert_eq!(still_there["data"]["stockQuantity"], 5);
}

#[tokio::test]
async fn module_status_stub_stays_public() {
    let app = common::spawn_app().await;

    // Deliberately unauthenticated: it returns a static string, no business
    // data (see the intentional-public list in `app::build_router`). Pinned
    // here so closing or widening it is a conscious decision rather than a
    // side effect.
    // Nested at "/" under the "/api/inventory" prefix, which axum resolves as
    // the prefix itself — there is no trailing-slash form of this route.
    let (status, json) = send_anon(&app.router, "GET", "/api/inventory", None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["success"], true);
}

#[tokio::test]
async fn inventory_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send_anon(&app.router, "GET", "/api/inventory/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn inventory_stats_endpoint_returns_totals_and_low_stock_alerts() {
    // `spawn_app`'s DB is shared across the suite, and `/inventory/stats`
    // aggregates the WHOLE products/categories/subcategories collections
    // with no filter — connect an isolated throwaway database instead, same
    // as the billing/repairs/print-jobs stats tests.
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = simplebash_pos_backend::clients::mongo::connect(&common::mongo_uri(), &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    let (category_key, subcategory_key) = seed_category_with_subcategory(&db, "Widgets").await;
    seed_subcategory(&db, &category_key, "Gadgets").await;
    let (category_key_2, subcategory_key_2) = seed_category_with_subcategory(&db, "Parts").await;

    // 2 products at/below threshold (low stock), 1 comfortably above, 1
    // soft-deleted (must be excluded entirely from both counts).
    seed_product_with(&db, &category_key, &subcategory_key, 2, 5, false).await;
    seed_product_with(&db, &category_key, &subcategory_key, 5, 5, false).await;
    seed_product_with(&db, &category_key_2, &subcategory_key_2, 50, 5, false).await;
    seed_product_with(&db, &category_key_2, &subcategory_key_2, 1, 5, true).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    let stats =
        simplebash_pos_backend::modules::inventory::service::stats::get_inventory_stats(&db_handle)
            .await
            .expect("get_inventory_stats should succeed");

    assert_eq!(
        stats.total_items, 3,
        "the soft-deleted product must be excluded"
    );
    assert_eq!(stats.low_stock_alerts, 2);
    assert_eq!(stats.total_categories, 2);
    assert_eq!(stats.total_subcategories, 3);

    db.drop().await.ok();
}
