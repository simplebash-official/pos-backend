// A receipt is one unit of work: the receipt row, its lines, their stock batches, the stock
// movements, the product totals and the serials commit together or not at all.
// Two ways a receipt fails are exercised: a serial listed twice (refused before anything is
// written), and two receipts racing for the same serial number, where the loser passes the
// pre-check and fails on the unique index after its receipt, line, batch and movement are
// already written. Nothing of the loser may remain. Runs on SQLite and on a throwaway Atlas
// database.

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
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": product_key,
                "quantity": serials.len(),
                "unitCostCents": 30000,
                "sellingPriceCents": 50000,
                "serialNumbers": serials,
            }],
        })
    };

    // A serial listed twice is refused before anything is written.
    let twin = format!("TWIN-{tag}");
    let (status, body) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receive(vec![twin.clone(), twin.clone()])),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "SERIAL_ALREADY_EXISTS", "{body}");
    assert_nothing_received(router, &admin, &product_id, &product_key, &supplier_key).await;

    // The same serial list without the repeat commits everything.
    let (status, body) = send(
        router,
        "POST",
        "/api/purchase-receipts",
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

    // Two receipts race for the same serial. Whichever loses may pass the pre-check, so it can
    // fail only after its receipt, line, batch and movement are written: none of that may stay.
    let racing = vec![format!("RACE-{tag}")];
    let (first, second) = tokio::join!(
        send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receive(racing.clone())),
            &admin
        ),
        send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receive(racing.clone())),
            &admin
        ),
    );
    let outcomes = [first.0, second.0];
    assert_eq!(
        outcomes
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1,
        "exactly one racing receipt may win: {outcomes:?} {} {}",
        first.1,
        second.1
    );
    let (_, product) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        product["data"]["stockQuantity"], 3,
        "the loser must not add stock"
    );
    let (_, serials) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_key}/serials"),
        None,
        &admin,
    )
    .await;
    assert_eq!(serials["data"]["items"].as_array().unwrap().len(), 3);
    let (_, batches) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_key}/batches"),
        None,
        &admin,
    )
    .await;
    assert_eq!(batches["data"]["items"].as_array().unwrap().len(), 2);
    let (_, receipts) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        receipts["data"]["items"].as_array().unwrap().len(),
        2,
        "the loser's receipt must not remain"
    );
}

/// The product has no stock, batch, serial, movement or receipt.
async fn assert_nothing_received(
    router: &axum::Router,
    admin: &str,
    product_id: &str,
    product_key: &str,
    supplier_key: &str,
) {
    let (status, product) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{product}");
    assert_eq!(product["data"]["stockQuantity"], 0, "stock must not move");
    for (path, field, what) in [
        (
            format!("/api/inventory/products/{product_key}/batches"),
            "items",
            "batch",
        ),
        (
            format!("/api/inventory/products/{product_key}/serials"),
            "items",
            "serial",
        ),
        (
            format!("/api/inventory/products/{product_id}/movements"),
            "movements",
            "stock movement",
        ),
        (
            format!("/api/purchases?supplierKey={supplier_key}"),
            "items",
            "purchase line",
        ),
        (
            format!("/api/purchase-receipts?supplierKey={supplier_key}"),
            "items",
            "receipt",
        ),
    ] {
        let (_, body) = send(router, "GET", &path, None, admin).await;
        assert_eq!(
            body["data"][field],
            json!([]),
            "no {what} may remain: {body}"
        );
    }
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
