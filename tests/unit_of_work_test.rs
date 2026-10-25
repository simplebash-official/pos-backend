// A purchase is one unit of work: the purchase row, its stock batch, the stock
// movement, the product total and the serials commit together or not at all.
// The failure injected here is a serial number listed twice in one receipt:
// the pre-check passes (neither copy is tracked yet) and the second insert hits
// the unique index after the purchase, the batch, the movement and the first
// serial are already written. Nothing of the receipt may remain. Runs on SQLite
// and on a throwaway Atlas database.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::{Value, json};
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

async fn scenario(app: &common::TestApp) {
    let router = &app.router;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];

    let (status, category) = send(
        router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": format!("Uow Cat {tag}"), "icon": "box", "color": "blue" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let category_key = category["data"]["key"].as_str().unwrap().to_string();
    let (status, sub) = send(
        router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Phones" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sub}");
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, supplier) = send(
        router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Uow Supplier {tag}"),
            "contactPerson": "Nimal",
            "primaryPhone": "0771234567",
            "address": "Colombo",
            "suppliedCategories": [category_key],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{supplier}");
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
    let (status, product) = send(
        router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": format!("Uow Phone {tag}"),
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "minStockThreshold": 1,
            "isSerialized": true,
            "warrantyMonths": 12,
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let product_id = product["data"]["id"].as_str().unwrap().to_string();
    let product_key = product["data"]["key"].as_str().unwrap().to_string();

    let receive = |serials: Vec<String>| {
        json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "quantity": serials.len(),
            "unitCostCents": 30000,
            "sellingPriceCents": 50000,
            "date": chrono::Utc::now().to_rfc3339(),
            "serialNumbers": serials,
        })
    };

    // The receipt dies on its second serial; everything it wrote must be gone.
    let twin = format!("TWIN-{tag}");
    let (status, body) = send(
        router,
        "POST",
        "/api/purchases",
        Some(receive(vec![twin.clone(), twin.clone()])),
        &admin,
    )
    .await;
    assert!(
        !status.is_success(),
        "a receipt with a repeated serial must fail: {body}"
    );

    let (status, product) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{product}");
    assert_eq!(product["data"]["stockQuantity"], 0, "stock must not move");

    let (_, batches) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_key}/batches"),
        None,
        &admin,
    )
    .await;
    assert_eq!(batches["data"]["items"], json!([]), "no batch may remain");

    let (_, serials) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_key}/serials"),
        None,
        &admin,
    )
    .await;
    assert_eq!(serials["data"]["items"], json!([]), "no serial may remain");

    let (_, movements) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_id}/movements"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        movements["data"]["movements"],
        json!([]),
        "no stock movement may remain"
    );

    let (_, purchases) = send(
        router,
        "GET",
        &format!("/api/purchases?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        purchases["data"]["items"],
        json!([]),
        "no purchase row may remain"
    );

    // The same serial list without the repeat commits everything.
    let (status, body) = send(
        router,
        "POST",
        "/api/purchases",
        Some(receive(vec![format!("A-{tag}"), format!("B-{tag}")])),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, product) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(product["data"]["stockQuantity"], 2);
    let (_, serials) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_key}/serials"),
        None,
        &admin,
    )
    .await;
    assert_eq!(serials["data"]["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_failed_receipt_leaves_nothing_behind_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn a_failed_receipt_leaves_nothing_behind_on_mongo() {
    let app = common::spawn_app_isolated().await;
    // The unique serial index (created at start-up in production) is what makes
    // the second twin fail after the first one was written.
    app.db
        .collection::<mongodb::bson::Document>("product_serials")
        .create_index(
            mongodb::IndexModel::builder()
                .keys(mongodb::bson::doc! { "serial_number": 1 })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(true)
                        .build(),
                )
                .build(),
        )
        .await
        .unwrap();
    scenario(&app).await;
    app.db.drop().await.ok();
}
