mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use simplebash_pos_backend::seeds;
use serde_json::json;
use tower::ServiceExt;

async fn send_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }

    let request = if let Some(json_body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&json_body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let json: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null)
    };

    (status, json)
}

#[tokio::test]
async fn test_seed_all_populates_database_and_allows_login() {
    let app = common::spawn_app_sqlite().await;

    // 1. Run master seed
    let summary = seeds::seed_all(&app.db_handle)
        .await
        .expect("seed_all should succeed");

    assert!(summary.admin.created);
    assert_eq!(summary.admin.email, "admin@pos.com");
    assert!(summary.providers.categories_created >= 3);
    assert_eq!(summary.suppliers.suppliers_created, 5);
    assert_eq!(summary.customers.customers_created, 20);
    assert!(summary.inventory.products_created >= 75);

    // 2. Login with seeded admin credentials
    let (login_status, login_res) = send_request(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({
            "email": "admin@pos.com",
            "password": "admin@1234"
        })),
    )
    .await;

    if login_status != StatusCode::OK {
        panic!("login failed: status={login_status}, response={login_res}");
    }
    assert_eq!(login_status, StatusCode::OK);
    assert_eq!(login_res["success"], true);

    let token = login_res["data"]["token"]
        .as_str()
        .expect("login response must contain JWT token");

    // 3. Verify /auth/me returns the seeded admin
    let (me_status, me_res) =
        send_request(&app.router, "GET", "/api/auth/me", Some(token), None).await;

    assert_eq!(me_status, StatusCode::OK);
    assert_eq!(me_res["data"]["email"], "admin@pos.com");
    assert_eq!(me_res["data"]["role"], "admin");
    assert_eq!(me_res["data"]["name"], "System Admin");

    // 4. Verify categories are populated
    let (cat_status, cat_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/categories",
        Some(token),
        None,
    )
    .await;

    assert_eq!(cat_status, StatusCode::OK);
    let categories = cat_res["data"]["categories"]
        .as_array()
        .expect("categories list");
    assert!(categories.len() >= 3, "expected at least 3 categories");

    // 5. Verify suppliers are populated
    let (sup_status, sup_res) =
        send_request(&app.router, "GET", "/api/suppliers", Some(token), None).await;

    assert_eq!(sup_status, StatusCode::OK);
    let suppliers = sup_res["data"]["suppliers"]
        .as_array()
        .expect("suppliers list");
    assert_eq!(suppliers.len(), 5, "expected exactly 5 seeded suppliers");

    // 6. Verify customers are populated
    let (cust_status, cust_res) = send_request(
        &app.router,
        "GET",
        "/api/customers?limit=100",
        Some(token),
        None,
    )
    .await;

    assert_eq!(cust_status, StatusCode::OK);
    let customers = cust_res["data"]["customers"]
        .as_array()
        .expect("customers list");
    assert_eq!(customers.len(), 20, "expected 20 seeded customers");
    assert_eq!(cust_res["data"]["total"], 20);

    // 7. Verify inventory products are populated
    let (prod_status, prod_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/products?limit=100",
        Some(token),
        None,
    )
    .await;

    assert_eq!(prod_status, StatusCode::OK);
    let products = prod_res["data"]["items"].as_array().expect("products list");
    assert!(products.len() >= 75, "expected at least 75 seeded products");
}

#[tokio::test]
async fn test_seeded_data_works_in_sale_transaction() {
    let app = common::spawn_app_sqlite().await;

    // Seed database
    seeds::seed_all(&app.db_handle)
        .await
        .expect("seed_all should succeed");

    // Login as admin
    let (_, login_res) = send_request(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({
            "email": "admin@pos.com",
            "password": "admin@1234"
        })),
    )
    .await;

    let token = login_res["data"]["token"].as_str().unwrap();

    // Fetch product
    let (_, prod_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/products?limit=1",
        Some(token),
        None,
    )
    .await;
    let product_key = prod_res["data"]["items"][0]["key"].as_str().unwrap();
    let unit_price_cents = prod_res["data"]["items"][0]["sellingPriceCents"]
        .as_i64()
        .unwrap();

    // Fetch customer
    let (_, cust_res) = send_request(
        &app.router,
        "GET",
        "/api/customers?limit=1",
        Some(token),
        None,
    )
    .await;
    let customer_key = cust_res["data"]["customers"][0]["key"].as_str().unwrap();

    // Complete a sale with the seeded product and customer
    let sale_payload = json!({
        "staff": {
            "cashierName": "System Admin"
        },
        "customer": {
            "customerKey": customer_key
        },
        "items": [
            {
                "productKey": product_key,
                "quantity": 2,
                "discountCents": 0,
                "sourceType": "retail"
            }
        ],
        "payment": {
            "paymentMethod": "cash",
            "isCredit": false,
            "amountReceivedCents": unit_price_cents * 2
        },
        "shopProfileSnapshot": {
            "name": "SimpleBash POS",
            "address": "Colombo"
        }
    });

    let (sale_status, sale_res) = send_request(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(token),
        Some(sale_payload),
    )
    .await;

    assert_eq!(
        sale_status,
        StatusCode::OK,
        "billing sale failed: {sale_res}"
    );
    assert_eq!(sale_res["success"], true);
    assert!(sale_res["data"]["invoice"]["key"].is_string());
}

#[tokio::test]
async fn test_seed_idempotency() {
    let app = common::spawn_app_sqlite().await;

    // Run 1
    let summary1 = seeds::seed_all(&app.db_handle)
        .await
        .expect("first seed_all must succeed");
    assert!(summary1.admin.created);

    // Run 2
    let summary2 = seeds::seed_all(&app.db_handle)
        .await
        .expect("second seed_all must succeed");
    assert!(!summary2.admin.created);
    assert_eq!(summary2.suppliers.suppliers_created, 0);
    assert_eq!(summary2.customers.customers_created, 0);
    assert_eq!(summary2.inventory.products_created, 0);
}
