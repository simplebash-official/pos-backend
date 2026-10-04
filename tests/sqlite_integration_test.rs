mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    app::{self, AppState},
    clients::{self, db::Db},
    core::{
        config::{Config, DatabaseType},
        constants::roles,
    },
    domain::users::Role,
};
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
        bind_addr: "127.0.0.1".to_string(),
        jwt_expiry_hours: 12,
        document_server_url: mock_doc_server_url,
        document_server_api_key: "test-doc-key".to_string(),
        generated_documents_dir: std::env::temp_dir().display().to_string(),
        return_window_days: 30,
        auto_seed: false,
        tenant_mode: simplebash_pos_backend::core::config::TenantMode::Single,
        cors_allowed_origins: Vec::new(),
        identity_jwks_url: None,
        identity_issuer: None,
        identity_tenant_id: None,
        provision_secret: None,
        app_env: "test".to_string(),
    };

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db_handle = Db::Sqlite(pool);
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
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

async fn execute_raw(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

async fn send_authed_raw(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    execute_raw(router, builder.body(Body::empty()).unwrap()).await
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

    // Create 2 additional products to test multi-item cart checkout concurrency
    let (status, prod2_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "USB Ring Light 10-inch with Phone Holder",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 2499,
            "costPriceCents": 1500,
            "stockQuantity": 20,
            "minStockThreshold": 5
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id_2 = prod2_res["data"]["id"].as_str().unwrap().to_string();
    let product_key_2 = prod2_res["data"]["key"].as_str().unwrap().to_string();

    let (status, prod3_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Silicone Sport Band 20mm (Black)",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 799,
            "costPriceCents": 300,
            "stockQuantity": 15,
            "minStockThreshold": 2
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id_3 = prod3_res["data"]["id"].as_str().unwrap().to_string();
    let product_key_3 = prod3_res["data"]["key"].as_str().unwrap().to_string();

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

    // 8. Billing Sale Completion (Multi-item cart: Invoice + Payment + Stock decrement for 3 items)
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
                },
                {
                    "productKey": product_key_2,
                    "quantity": 1,
                    "discountCents": 0,
                    "sourceType": "retail"
                },
                {
                    "productKey": product_key_3,
                    "quantity": 3,
                    "discountCents": 0,
                    "sourceType": "retail"
                }
            ],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": 300000
            },
            "shopProfileSnapshot": {
                "name": "SimpleBash Shop",
                "address": "Colombo"
            }
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "billing sale failed: {sale_res}");
    let warnings = sale_res["data"]["warnings"].as_array().unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "multi-item sale must complete with 0 warnings, got: {warnings:?}"
    );

    let invoice_key = sale_res["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let invoice_number = sale_res["data"]["invoice"]["invoiceNumber"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        invoice_number.starts_with("INV-"),
        "Invoice number should start with INV-"
    );

    // Check that stock was decremented accurately across all 3 products:
    // Product 1: 30 -> 28
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

    // Product 2: 20 -> 19
    let (status, prod2_after_sale) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id_2}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prod2_after_sale["data"]["stockQuantity"], 19);

    // Product 3: 15 -> 12
    let (status, prod3_after_sale) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id_3}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prod3_after_sale["data"]["stockQuantity"], 12);

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
    let credit_note_key = cn_res["data"]["key"].as_str().unwrap().to_string();
    let credit_note_number = cn_res["data"]["creditNoteNumber"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        credit_note_number.starts_with("CN-"),
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

    // 10. Document Generation & Caching (Invoice & Credit Note)
    // 10a. Render A4 Invoice by invoice_key
    let (status, headers, a4_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}/documents/a4-invoice"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "application/pdf",
        "expected application/pdf content-type"
    );
    assert!(!a4_bytes.is_empty(), "PDF bytes should not be empty");

    // 10b. Render A4 Invoice by invoice_number (verifies lookup by sequence number & cache hit)
    let (status, headers, a4_cached_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_number}/documents/a4-invoice"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/pdf");
    assert_eq!(a4_cached_bytes, a4_bytes, "cached PDF bytes should match");

    // 10c. Render Thermal Receipt
    let (status, headers, receipt_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}/documents/thermal-receipt?paperWidthMm=80"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/pdf");
    assert!(!receipt_bytes.is_empty());

    // 10d. Render Credit Note by credit_note_key
    let (status, headers, cn_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/credit-notes/{credit_note_key}/documents/credit-note"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/pdf");
    assert!(!cn_bytes.is_empty());

    // 10e. Render Credit Note by credit_note_number
    let (status, headers, cn_cached_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/credit-notes/{credit_note_number}/documents/credit-note"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/pdf");
    assert_eq!(cn_cached_bytes, cn_bytes);

    // 11. Reports Engine Feeds
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

    // 12. Sync Changes
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

#[tokio::test]
async fn test_sqlite_legacy_user_id_schema_migration() {
    dotenvy::dotenv().ok();
    let db_path = std::env::temp_dir().join(format!("pos_legacy_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let options: sqlx::sqlite::SqliteConnectOptions = db_url.parse().unwrap();
    let options = options.create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // 1. Artificially create legacy schema containing user_id TEXT NOT NULL
    sqlx::query(
        r#"
        CREATE TABLE login_sessions (
            key TEXT PRIMARY KEY,
            id TEXT NOT NULL,
            user_key TEXT NOT NULL,
            user_id TEXT NOT NULL,
            name_at_login TEXT NOT NULL,
            email_at_login TEXT NOT NULL,
            role_at_login TEXT NOT NULL,
            ip_address TEXT,
            user_agent TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // 2. Run init_db which must automatically heal and drop legacy user_id
    clients::sqlite::init_db(&pool).await.unwrap();

    // 3. Verify user_id is dropped
    let columns: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('login_sessions')")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(
        !columns.iter().any(|(_, name)| name == "user_id"),
        "legacy user_id column should have been dropped by migrate_schema"
    );

    // 4. Test insert into login_sessions with current schema (no user_id) succeeds without constraint failure
    sqlx::query(
        r#"
        INSERT INTO login_sessions (
            id, key, user_key, name_at_login, email_at_login, role_at_login,
            ip_address, user_agent, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("507f1f77bcf86cd799439011")
    .bind("ses_test123456789")
    .bind("usr_test123456789")
    .bind("Admin Test")
    .bind("admin@test.com")
    .bind("admin")
    .bind("127.0.0.1")
    .bind("TestAgent")
    .bind("2026-09-06T10:00:00Z")
    .bind("2026-09-06T10:00:00Z")
    .execute(&pool)
    .await
    .expect("INSERT INTO login_sessions without user_id must succeed after migration");

    // Clean up temp database
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_generated_documents_schema_migration() {
    dotenvy::dotenv().ok();
    let db_path =
        std::env::temp_dir().join(format!("pos_gen_doc_migration_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let options: sqlx::sqlite::SqliteConnectOptions = db_url.parse().unwrap();
    let options = options.create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // 1. Artificially create legacy generated_documents schema missing entity_key, document_type, file_size_bytes
    sqlx::query(
        r#"
        CREATE TABLE generated_documents (
            key TEXT PRIMARY KEY,
            id TEXT NOT NULL,
            template_key TEXT NOT NULL,
            template_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // 2. Run init_db which must automatically heal and add missing columns & index
    clients::sqlite::init_db(&pool).await.unwrap();

    // 3. Verify entity_key, document_type, and file_size_bytes are added
    let columns: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('generated_documents')")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(
        columns.iter().any(|(_, name)| name == "entity_key"),
        "entity_key column should have been added by migrate_schema"
    );
    assert!(
        columns.iter().any(|(_, name)| name == "document_type"),
        "document_type column should have been added by migrate_schema"
    );
    assert!(
        columns.iter().any(|(_, name)| name == "file_size_bytes"),
        "file_size_bytes column should have been added by migrate_schema"
    );

    // 4. Test SELECT query with all columns works without database error
    let row_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM generated_documents
        WHERE entity_key = 'inv_123' AND document_type = 'a4-invoice'
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("SELECT on generated_documents with entity_key must succeed");
    assert_eq!(row_count, 0);

    // 5. Test INSERT into generated_documents with all 10 columns succeeds
    sqlx::query(
        r#"
        INSERT INTO generated_documents (
            key, id, entity_key, document_type, template_name, template_key,
            file_path, file_size_bytes, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("doc_test123456789")
    .bind("507f1f77bcf86cd799439011")
    .bind("inv_test123456789")
    .bind("a4-invoice")
    .bind("a4-invoice")
    .bind("tpl_a4_invoice")
    .bind("inv_test123456789/a4-invoice.pdf")
    .bind(1024_i64)
    .bind("2026-09-06T10:00:00Z")
    .bind("2026-09-06T10:00:00Z")
    .execute(&pool)
    .await
    .expect("INSERT INTO generated_documents must succeed after migration");

    // Clean up temp database
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_repeated_failed_logins_are_throttled() {
    let ctx = setup_sqlite_app().await;
    // Unique per run: the limiter is process-wide, so a shared email could be
    // pushed over the limit by other tests' failures.
    let email = format!("brute-{}@example.test", Uuid::new_v4());
    let attempt = || {
        Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "email": email, "password": "wrong-password-123" }).to_string(),
            ))
            .unwrap()
    };

    for i in 0..10 {
        let (status, body) = execute(&ctx.router, attempt()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {i}: {body}");
    }
    let (status, body) = execute(&ctx.router, attempt()).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "TOO_MANY_LOGIN_ATTEMPTS");
}

#[tokio::test]
async fn test_sqlite_deactivating_a_user_revokes_their_live_token() {
    let ctx = setup_sqlite_app().await;
    let admin = admin_token(&ctx.config);
    let email = format!("staff-{}@example.test", Uuid::new_v4());
    let password = "Staff-Password-123";

    let (status, body) = send_authed(
        &ctx.router,
        "POST",
        "/api/users",
        Some(json!({ "name": "Staff Member", "email": email, "password": password, "role": "staff" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let user_id = body["data"]["id"].as_str().expect("user id").to_string();

    let login = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .unwrap();
    let (status, body) = execute(&ctx.router, login).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let staff_token = body["data"]["token"].as_str().expect("token").to_string();

    let (status, body) =
        send_authed(&ctx.router, "GET", "/api/customers", None, &staff_token).await;
    assert_eq!(status, StatusCode::OK, "before deactivation: {body}");

    let (status, body) = send_authed(
        &ctx.router,
        "PATCH",
        &format!("/api/users/{user_id}"),
        Some(json!({ "isActive": false })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The token itself is still unexpired, but the account behind it is not
    // active any more: every authenticated route must refuse it now.
    let (status, body) =
        send_authed(&ctx.router, "GET", "/api/customers", None, &staff_token).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "after deactivation: {body}"
    );
    assert_eq!(body["code"], "USER_INACTIVE");
}
