//! Lifecycle tests.

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, StatusCode};
use serde_json::json;

use simplebash_pos_backend::clients;

use tower::ServiceExt;
use uuid::Uuid;

use crate::helpers::{admin_token, execute, send_authed, setup_sqlite_app};

async fn execute_raw(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds")
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
    execute_raw(
        router,
        builder
            .body(Body::empty())
            .expect("the request is well formed"),
    )
    .await
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
            .expect("the request is well formed"),
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
    let category_key = cat_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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
        .expect("the value is an array")
        .iter()
        .find(|s| s["name"] == "Smartphones")
        .expect("a match exists")["key"]
        .as_str()
        .expect("the value is a string")
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
            "openingStock": { "quantity": 10, "unitCostCents": 75000, "sellingPriceCents": 99900 },
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
    let product_id = prod_res["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product_key = prod_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let sku = prod_res["data"]["sku"]
        .as_str()
        .expect("the value is a string")
        .to_string();
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
            "openingStock": { "quantity": 20, "unitCostCents": 1500, "sellingPriceCents": 2499 },
            "minStockThreshold": 5
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id_2 = prod2_res["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product_key_2 = prod2_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, prod3_res) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Silicone Sport Band 20mm (Black)",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 15, "unitCostCents": 300, "sellingPriceCents": 799 },
            "minStockThreshold": 2
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id_3 = prod3_res["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product_key_3 = prod3_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // 4. Stock Adjustment
    let (status, adj_res) = send_authed(
        &ctx.router,
        "PATCH",
        &format!("/api/inventory/products/{product_id}/stock"),
        Some(json!({
            "delta": 15,
            "reason": "Restock test batch",
            "unitCostCents": 75000,
            "sellingPriceCents": 99900
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
    let customer_key = cust_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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
    let supplier_key = supp_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": "2026-09-06T00:00:00Z",
            "lines": [{
                "productKey": product_key,
                "quantity": 5,
                "lineTotalCents": 5 * 72000,
                "sellingPriceCents": 99900
            }]
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
                    "sellingPriceCents": 99900,
                    "quantity": 2,
                    "discountCents": 0,
                    "sourceType": "retail"
                },
                {
                    "productKey": product_key_2,
                    "sellingPriceCents": 2499,
                    "quantity": 1,
                    "discountCents": 0,
                    "sourceType": "retail"
                },
                {
                    "productKey": product_key_3,
                    "sellingPriceCents": 799,
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
    let warnings = sale_res["data"]["warnings"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(
        warnings.len(),
        0,
        "multi-item sale must complete with 0 warnings, got: {warnings:?}"
    );

    let invoice_key = sale_res["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let invoice_number = sale_res["data"]["invoice"]["invoiceNumber"]
        .as_str()
        .expect("the value is a string")
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
    let credit_note_key = cn_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let credit_note_number = cn_res["data"]["creditNoteNumber"]
        .as_str()
        .expect("the value is a string")
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
        &format!("/api/billing/invoices/{invoice_key}/documents/a4-invoice?design=current"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").expect("the key is present"),
        "application/pdf",
        "expected application/pdf content-type"
    );
    assert!(!a4_bytes.is_empty(), "PDF bytes should not be empty");

    // 10b. Render A4 Invoice by invoice_number (verifies lookup by sequence number & cache hit)
    let (status, headers, a4_cached_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_number}/documents/a4-invoice?design=current"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").expect("the key is present"),
        "application/pdf"
    );
    assert_eq!(a4_cached_bytes, a4_bytes, "cached PDF bytes should match");

    // 10c. Render Thermal Receipt
    let (status, headers, receipt_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}/documents/thermal-receipt?paperWidthMm=80&design=current"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").expect("the key is present"),
        "application/pdf"
    );
    assert!(!receipt_bytes.is_empty());

    // 10d. Render Credit Note by credit_note_key
    let (status, headers, cn_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!(
            "/api/billing/credit-notes/{credit_note_key}/documents/credit-note?design=current"
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").expect("the key is present"),
        "application/pdf"
    );
    assert!(!cn_bytes.is_empty());

    // 10e. Render Credit Note by credit_note_number
    let (status, headers, cn_cached_bytes) = send_authed_raw(
        &ctx.router,
        "GET",
        &format!(
            "/api/billing/credit-notes/{credit_note_number}/documents/credit-note?design=current"
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").expect("the key is present"),
        "application/pdf"
    );
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
    let changes = sync_res["data"]["changes"]
        .as_object()
        .expect("the value is an object");
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

    let options: sqlx::sqlite::SqliteConnectOptions = db_url.parse().expect("the text parses");
    let options = options.create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options)
        .await
        .expect("the awaited step succeeds");

    // 1. Artificially create legacy schema containing user_id TEXT NOT NULL
    sqlx::query(
        r#"
        CREATE TABLE login_sessions (
            key TEXT PRIMARY KEY,
            id TEXT NOT NULL,
            user_key TEXT NOT NULL,
            user_id TEXT NOT NULL,
            name_at_login TEXT NOT NULL,
            username_at_login TEXT NOT NULL,
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
    .expect("the awaited step succeeds");

    // 2. Run init_db which must automatically heal and drop legacy user_id
    clients::sqlite::init_db(&pool)
        .await
        .expect("the awaited step succeeds");

    // 3. Verify user_id is dropped
    let columns: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('login_sessions')")
            .fetch_all(&pool)
            .await
            .expect("the awaited step succeeds");

    assert!(
        !columns.iter().any(|(_, name)| name == "user_id"),
        "legacy user_id column should have been dropped by migrate_schema"
    );

    // 4. Test insert into login_sessions with current schema (no user_id) succeeds without constraint failure
    sqlx::query(
        r#"
        INSERT INTO login_sessions (
            id, key, user_key, name_at_login, username_at_login, role_at_login,
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
