mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
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
async fn test_setup_status_on_fresh_database() {
    let app = common::spawn_app_sqlite().await;

    let (status, res) =
        send_request(&app.router, "GET", "/api/system/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["success"], true);
    assert_eq!(res["data"]["setupCompleted"], false);
    assert_eq!(res["data"]["isFirstRun"], true);
    assert!(res["data"]["installationId"].is_string());
    assert!(res["data"]["installedAt"].is_string());
}

#[tokio::test]
async fn test_setup_without_sample_data_creates_clean_database() {
    let app = common::spawn_app_sqlite().await;

    // 1. Initial setup with load_sample_data = false
    let (setup_status, setup_res) = send_request(
        &app.router,
        "POST",
        "/api/system/setup",
        None,
        Some(json!({
            "loadSampleData": false,
            "adminName": "Clean Shop Owner",
            "adminPassword": "password123"
        })),
    )
    .await;

    assert_eq!(setup_status, StatusCode::OK);
    assert_eq!(setup_res["success"], true);
    assert_eq!(setup_res["data"]["setupCompleted"], true);
    assert_eq!(setup_res["data"]["sampleDataLoaded"], false);
    assert_eq!(setup_res["data"]["adminUsername"], "admin");

    let token = setup_res["data"]["token"]
        .as_str()
        .expect("token should be present for auto-login");
    assert!(!token.is_empty());

    // 2. Verify all catalog and operations tables remain clean and empty
    let (prod_status, prod_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/products",
        Some(token),
        None,
    )
    .await;
    assert_eq!(prod_status, StatusCode::OK);
    assert_eq!(prod_res["data"]["items"].as_array().unwrap().len(), 0);

    let (cust_status, cust_res) =
        send_request(&app.router, "GET", "/api/customers", Some(token), None).await;
    assert_eq!(cust_status, StatusCode::OK);
    assert_eq!(cust_res["data"]["customers"].as_array().unwrap().len(), 0);

    let (sup_status, sup_res) =
        send_request(&app.router, "GET", "/api/suppliers", Some(token), None).await;
    assert_eq!(sup_status, StatusCode::OK);
    assert_eq!(sup_res["data"]["suppliers"].as_array().unwrap().len(), 0);

    let (cat_status, cat_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/categories",
        Some(token),
        None,
    )
    .await;
    assert_eq!(cat_status, StatusCode::OK);
    assert_eq!(cat_res["data"]["categories"].as_array().unwrap().len(), 0);

    // 3. Status endpoint now reflects setup completed with clean tables
    let (status, status_res) =
        send_request(&app.router, "GET", "/api/system/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(status_res["data"]["setupCompleted"], true);
    assert_eq!(status_res["data"]["isFirstRun"], false);
    assert_eq!(status_res["data"]["sampleDataLoaded"], false);

    // 4. Repeated setup call must be rejected with 409 Conflict
    let (dup_status, dup_res) = send_request(
        &app.router,
        "POST",
        "/api/system/setup",
        None,
        Some(json!({
            "loadSampleData": true,
            "adminPassword": "password123"
        })),
    )
    .await;
    assert_eq!(dup_status, StatusCode::CONFLICT);
    assert_eq!(dup_res["code"], "SETUP_ALREADY_COMPLETED");
}

#[tokio::test]
async fn test_setup_with_sample_data_populates_catalog_and_operations() {
    let app = common::spawn_app_sqlite().await;

    // 1. Initial setup with load_sample_data = true
    let (setup_status, setup_res) = send_request(
        &app.router,
        "POST",
        "/api/system/setup",
        None,
        Some(json!({
            "loadSampleData": true,
            "adminName": "Demo Admin",
            "adminPassword": "password123"
        })),
    )
    .await;

    assert_eq!(setup_status, StatusCode::OK);
    assert_eq!(setup_res["success"], true);
    assert_eq!(setup_res["data"]["setupCompleted"], true);
    assert_eq!(setup_res["data"]["sampleDataLoaded"], true);

    let token = setup_res["data"]["token"]
        .as_str()
        .expect("token should be present for auto-login");

    // 2. Verify products, categories, customers, and suppliers are populated
    let (prod_status, prod_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/products",
        Some(token),
        None,
    )
    .await;
    assert_eq!(prod_status, StatusCode::OK);
    assert!(prod_res["data"]["pagination"]["total"].as_u64().unwrap() >= 50);
    assert!(prod_res["data"]["items"].as_array().unwrap().len() >= 20);

    let (cust_status, cust_res) =
        send_request(&app.router, "GET", "/api/customers", Some(token), None).await;
    assert_eq!(cust_status, StatusCode::OK);
    assert_eq!(cust_res["data"]["total"], 20);
    assert_eq!(cust_res["data"]["customers"].as_array().unwrap().len(), 10);

    let (sup_status, sup_res) =
        send_request(&app.router, "GET", "/api/suppliers", Some(token), None).await;
    assert_eq!(sup_status, StatusCode::OK);
    assert_eq!(sup_res["data"]["suppliers"].as_array().unwrap().len(), 5);

    let (cat_status, cat_res) = send_request(
        &app.router,
        "GET",
        "/api/inventory/categories",
        Some(token),
        None,
    )
    .await;
    assert_eq!(cat_status, StatusCode::OK);
    assert!(cat_res["data"]["categories"].as_array().unwrap().len() >= 3);

    // 3. Status endpoint reflects sample data loaded
    let (status, status_res) =
        send_request(&app.router, "GET", "/api/system/setup-status", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(status_res["data"]["setupCompleted"], true);
    assert_eq!(status_res["data"]["sampleDataLoaded"], true);
}

#[tokio::test]
async fn test_setup_with_snake_case_payload_succeeds() {
    let app = common::spawn_app_sqlite().await;

    // Test that backend gracefully accepts snake_case fields sent by frontend
    let (setup_status, setup_res) = send_request(
        &app.router,
        "POST",
        "/api/system/setup",
        None,
        Some(json!({
            "load_sample_data": false,
            "admin_name": "Snake Case Admin",
            "admin_password": "password123"
        })),
    )
    .await;

    assert_eq!(setup_status, StatusCode::OK);
    assert_eq!(setup_res["success"], true);
    assert_eq!(setup_res["data"]["setupCompleted"], true);
    assert_eq!(setup_res["data"]["sampleDataLoaded"], false);
    assert_eq!(setup_res["data"]["adminUsername"], "admin");
}

#[tokio::test]
async fn test_setup_requires_an_explicit_admin_password() {
    let app = common::spawn_app_sqlite().await;

    for body in [
        json!({ "loadSampleData": false }),
        json!({ "loadSampleData": false, "adminPassword": "short" }),
    ] {
        let (status, res) =
            send_request(&app.router, "POST", "/api/system/setup", None, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{res}");
    }

    // A rejected attempt must not consume the one-time setup.
    let (_, status_res) =
        send_request(&app.router, "GET", "/api/system/setup-status", None, None).await;
    assert_eq!(status_res["data"]["setupCompleted"], false);

    let (ok_status, _) = send_request(
        &app.router,
        "POST",
        "/api/system/setup",
        None,
        Some(json!({
            "loadSampleData": false,
            "adminPassword": "longenough1"
        })),
    )
    .await;
    assert_eq!(ok_status, StatusCode::OK);
}
