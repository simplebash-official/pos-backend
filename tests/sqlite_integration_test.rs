mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jana2u_pos_backend::{
    app::{self, AppState},
    clients::{self, db::Db},
    core::{
        config::{Config, DatabaseType},
        constants::roles,
    },
    domain::users::Role,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct SqliteTestContext {
    router: axum::Router,
    config: Arc<Config>,
    db_path: std::path::PathBuf,
}

impl Drop for SqliteTestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let shm = self.db_path.with_extension("db-shm");
        let wal = self.db_path.with_extension("db-wal");
        let _ = std::fs::remove_file(shm);
        let _ = std::fs::remove_file(wal);
    }
}

async fn setup_sqlite_app() -> SqliteTestContext {
    dotenvy::dotenv().ok();
    let db_path = std::env::temp_dir().join(format!("pos_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = clients::sqlite::connect(&db_url)
        .await
        .expect("failed to initialize SQLite test database");

    let mock_doc_server_url = common::start_mock_document_server().await;

    let config = Config {
        database_type: DatabaseType::Sqlite,
        database_url: db_url,
        mongodb_uri: String::new(),
        mongodb_db_name: String::new(),
        jwt_secret: "test-sqlite-jwt-secret-key-that-is-long-enough".to_string(),
        port: 8080,
        jwt_expiry_hours: 12,
        document_server_url: mock_doc_server_url,
        document_server_api_key: "test-doc-key".to_string(),
        generated_documents_dir: std::env::temp_dir().display().to_string(),
        return_window_days: 30,
    };

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db_handle = Db::Sqlite(pool);
    let reports_engine = Arc::new(
        jana2u_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config: config.clone(),
        db: db_handle,
        document_server,
        reports_engine,
    };

    SqliteTestContext {
        router: app::build_router(state),
        config,
        db_path,
    }
}

async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
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

async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json");
    let body_bytes = match body {
        Some(val) => Body::from(serde_json::to_vec(&val).unwrap()),
        None => Body::empty(),
    };
    execute(router, builder.body(body_bytes).unwrap()).await
}

fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

#[tokio::test]
async fn test_sqlite_complete_pos_lifecycle() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);

    // 1. Health check
    let (status, health) = execute(
        &ctx.router,
        Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["data"]["status"], "ok");

    // 2. Category & Subcategory Creation
    let (status, cat_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": "Electronics",
            "icon": "devices",
            "color": "blue"
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "category creation failed: {cat_res}"
    );
    let category_key = cat_res["data"]["key"].as_str().unwrap().to_string();

    let (status, sub_res) = send_authed(
        &ctx.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({
            "name": "Smartphones"
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "subcategory creation failed: {sub_res}"
    );
    let subcategory_key = sub_res["data"]["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Smartphones")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();

    // 3. Product Creation with Automatic SKU Generation
    let (status, prod_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "iPhone 15 Pro",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 99900,
            "costPriceCents": 75000,
            "stockQuantity": 10,
            "minStockThreshold": 3
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "product creation failed: {prod_res}"
    );
    let product_id = prod_res["data"]["id"].as_str().unwrap().to_string();
    let product_key = prod_res["data"]["key"].as_str().unwrap().to_string();
    let sku = prod_res["data"]["sku"].as_str().unwrap().to_string();
    assert!(
        sku.starts_with("ELE-SMA-"),
        "SKU should follow derived prefix, got {sku}"
    );

    // 4. Stock Adjustment
    let (status, adj_res) = send_authed(
        &ctx.router,
        "PATCH",
        &format!("/api/inventory/products/{product_id}/stock"),
        Some(json!({
            "delta": 15,
            "reason": "Restock test batch"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "stock adjust failed: {adj_res}");
    assert_eq!(adj_res["data"]["stockQuantity"], 25);

    // 5. Customer Creation
    let (status, cust_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Jane Doe",
            "primaryPhone": "0771234567",
            "email": "jane@example.com"
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "customer creation failed: {cust_res}"
    );
    let customer_key = cust_res["data"]["key"].as_str().unwrap().to_string();

    // 6. Supplier Creation & Linking
    let (status, supp_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": "Apple Authorized Distributor",
            "contactPerson": "John Supplier",
            "primaryPhone": "0112345678",
            "address": "123 Tech Park, Colombo",
            "suppliedCategories": [category_key]
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "supplier creation failed: {supp_res}"
    );
    let supplier_key = supp_res["data"]["key"].as_str().unwrap().to_string();

    let (status, link_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "costPriceCents": 72000,
            "notes": "AAPL-IPH15P-BLK"
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "supplier product link failed: {link_res}"
    );

    // 7. Purchase Intake (Stock Intake)
    let (status, purch_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/purchases",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "quantity": 5,
            "unitCostCents": 72000,
            "date": "2026-09-06T00:00:00Z"
        })),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "purchase intake failed: {purch_res}"
    );

    // Check that product stock quantity is now 30 (10 initial + 15 adjustment + 5 purchase)
    let (status, prod_check) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prod_check["data"]["stockQuantity"], 30);

    // 8. Billing Sale Completion (Transaction: Invoice + Payment + Stock decrement)
    let (status, sale_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": {
                "cashierName": "Default Admin"
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
                "amountReceivedCents": 200000
            },
            "shopProfileSnapshot": {
                "name": "Jana2u Shop",
                "address": "Colombo"
            }
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "billing sale failed: {sale_res}");
    let invoice_key = sale_res["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        sale_res["data"]["invoice"]["invoiceNumber"]
            .as_str()
            .unwrap()
            .starts_with("INV-"),
        "Invoice number should start with INV-"
    );

    // Check that stock was decremented from 30 to 28
    let (status, prod_after_sale) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prod_after_sale["data"]["stockQuantity"], 28);

    // 9. Credit Note (Return 1 item, restores 1 stock)
    let (status, cn_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [
                {
                    "productKey": product_key,
                    "quantity": 1,
                    "condition": "resalable",
                    "reason": "defective"
                }
            ],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "credit note failed: {cn_res}");
    assert!(
        cn_res["data"]["creditNoteNumber"]
            .as_str()
            .unwrap()
            .starts_with("CN-"),
        "Credit note number should start with CN-"
    );

    // Stock should be incremented back to 29
    let (status, prod_after_return) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prod_after_return["data"]["stockQuantity"], 29);

    // 10. Reports Engine Feeds
    let (status, dashboard) = send_authed(
        &ctx.router,
        "GET",
        "/api/reports/dashboard?preset=today",
        None,
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "dashboard report failed: {dashboard}"
    );
    assert!(dashboard["data"]["totalRevenueCents"].as_i64().is_some());

    let (status, feed) = send_authed(
        &ctx.router,
        "GET",
        "/api/reports/engine/feed?section=all&preset=today",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "engine feed failed: {feed}");
    assert!(feed["data"]["overview"].is_object());
    assert!(feed["data"]["overview"]["summary"].is_object());

    // 11. Sync Changes
    let (status, sync_res) = send_authed(
        &ctx.router,
        "GET",
        "/api/sync/changes?since=2020-01-01T00:00:00Z",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sync changes failed: {sync_res}");
    let changes = sync_res["data"]["changes"].as_object().unwrap();
    assert!(
        !changes.is_empty(),
        "Changelog should contain sync mutations"
    );
}
