mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use simplebash_pos_backend::{
    core::{
        config::Config,
        constants::{codes, roles},
    },
    domain::users::Role,
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

async fn send_anon(
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

fn manager_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    )
}

fn staff_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

#[tokio::test]
async fn imports_routes_require_authentication() {
    let app = common::spawn_app().await;

    let (status, body) = send_anon(&app.router, "POST", "/api/imports", Some(json!({}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], codes::UNAUTHORIZED);

    let (status, body) = send_anon(&app.router, "GET", "/api/imports", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], codes::UNAUTHORIZED);

    let (status, body) = send_anon(&app.router, "GET", "/api/imports/imp_123456", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], codes::UNAUTHORIZED);
}

#[tokio::test]
async fn imports_reject_caller_without_inventory_write() {
    let app = common::spawn_app().await;

    let staff = staff_token(&app.config);
    let payload = json!({
        "target": "inventory",
        "fileName": "test.xlsx",
        "fileType": "xlsx",
        "rows": [
            {
                "name": "Test Item",
                "category": "Cat",
                "subcategory": "Sub",
                "sellingPrice": 100
            }
        ]
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/imports", Some(payload), &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], codes::PERMISSION_DENIED);
}

#[tokio::test]
async fn imports_process_valid_inventory_rows_and_record_batch() {
    let app = common::spawn_app().await;
    let manager = manager_token(&app.config);

    let unique_suffix = Uuid::new_v4().simple().to_string()[..8].to_string();
    let cat_name = format!("ImportCat {unique_suffix}");
    let subcat_name = format!("ImportSub {unique_suffix}");
    let item_1 = format!("Imported Widget A {unique_suffix}");
    let item_2 = format!("Imported Widget B {unique_suffix}");

    let payload = json!({
        "target": "inventory",
        "fileName": "stock_delivery.xlsx",
        "fileType": "xlsx",
        "fileSizeBytes": 2048,
        "rows": [
            {
                "name": item_1,
                "category": cat_name,
                "subcategory": subcat_name,
                "sellingPrice": 1500,
                "costPrice": 750,
                "stockQuantity": 10,
                "minStockThreshold": 3
            },
            {
                "name": item_2,
                "category": cat_name,
                "subcategory": subcat_name,
                "sellingPrice": 2200,
                "costPrice": 1100,
                "stockQuantity": 0,
                "minStockThreshold": 2
            }
        ],
        "options": {
            "autoCreateCategories": true,
            "autoGenerateBarcodes": true
        }
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/imports", Some(payload), &manager).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    let batch = &body["data"];
    assert_eq!(batch["target"], "inventory");
    assert_eq!(batch["fileName"], "stock_delivery.xlsx");
    assert_eq!(batch["totalRows"], 2);
    assert_eq!(batch["successfulRows"], 2);
    assert_eq!(batch["failedRows"], 0);
    assert_eq!(batch["status"], "completed");

    let batch_key = batch["key"].as_str().unwrap();
    assert!(batch_key.starts_with("imp_"));

    // Verify detail endpoint
    let (detail_status, detail_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/imports/{batch_key}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(detail_status, StatusCode::OK);
    assert_eq!(detail_body["data"]["key"], batch_key);
    assert_eq!(detail_body["data"]["successfulRows"], 2);

    // Verify list endpoint
    let (list_status, list_body) = send_authed(
        &app.router,
        "GET",
        "/api/imports?target=inventory&page=1&limit=10",
        None,
        &manager,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(
        list_body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["key"] == batch_key)
    );
}

#[tokio::test]
async fn imports_records_row_errors_for_invalid_rows() {
    let app = common::spawn_app().await;
    let manager = manager_token(&app.config);

    let unique_suffix = Uuid::new_v4().simple().to_string()[..8].to_string();
    let valid_name = format!("Valid Item {unique_suffix}");

    let payload = json!({
        "target": "inventory",
        "fileName": "corrupted_import.csv",
        "fileType": "csv",
        "rows": [
            // Row 1: Missing name
            {
                "name": "",
                "category": "Hardware",
                "subcategory": "Tools",
                "sellingPrice": 500
            },
            // Row 2: Invalid price (<= 0)
            {
                "name": "Invalid Price Item",
                "category": "Hardware",
                "subcategory": "Tools",
                "sellingPrice": -50
            },
            // Row 3: Valid row
            {
                "name": valid_name,
                "category": format!("Cat {unique_suffix}"),
                "subcategory": format!("Sub {unique_suffix}"),
                "sellingPrice": 1200
            }
        ],
        "options": {
            "autoCreateCategories": true
        }
    });

    let (status, body) =
        send_authed(&app.router, "POST", "/api/imports", Some(payload), &manager).await;
    assert_eq!(status, StatusCode::OK);

    let batch = &body["data"];
    assert_eq!(batch["totalRows"], 3);
    assert_eq!(batch["successfulRows"], 1);
    assert_eq!(batch["failedRows"], 2);
    assert_eq!(batch["status"], "partially_completed");

    let errors = batch["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["rowNumber"], 1);
    assert_eq!(errors[1]["rowNumber"], 2);
}
