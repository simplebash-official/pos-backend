//! Quotation documents tests.

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, StatusCode};
use simplebash_pos_backend::core::logging::SqlLogging;

use serde_json::json;

use simplebash_pos_backend::clients;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{call, create, date_in, request, setup, token, valid_body};
use tower::ServiceExt;
use uuid::Uuid;

/// The `quotations` table as it was created before it synced: no `version`,
/// `deleted_at` or `updated_by_device`.
const OLD_QUOTATIONS_TABLE: &str = r#"
CREATE TABLE quotations (
    key TEXT PRIMARY KEY, id TEXT NOT NULL, quotation_number TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL, customer_key TEXT, customer_name TEXT NOT NULL,
    customer_phone TEXT, customer_address TEXT, items TEXT NOT NULL,
    subtotal_cents INTEGER NOT NULL, discount_type TEXT NOT NULL,
    discount_value REAL NOT NULL, discount_cents INTEGER NOT NULL,
    total_cents INTEGER NOT NULL, valid_until TEXT NOT NULL, notes TEXT, terms TEXT,
    prepared_by TEXT, shop_profile_snapshot TEXT NOT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
)"#;

#[tokio::test]
async fn document_renders_a_pdf_with_the_quotation_template() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"]
        .as_str()
        .expect("the value is a string");
    let uri = format!("/api/quotations/{key}/document?design=current");

    let (status, headers, bytes) = request(&ctx.router, "GET", &uri, None, Some(&tok)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").expect("the key is present"),
        "application/pdf"
    );
    // The mock echoes which template key it rendered.
    assert!(
        String::from_utf8_lossy(&bytes).contains("tpl_quotation"),
        "rendered with the wrong template"
    );

    // Reprinting is served from the cache and is identical.
    let (_, _, again) = request(&ctx.router, "GET", &uri, None, Some(&tok)).await;
    assert_eq!(bytes, again);

    // A cancelled quotation still prints (stamped), under its own cache key.
    call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{key}/cancel"),
        None,
        &tok,
    )
    .await;
    let (status, _, cancelled) = request(&ctx.router, "GET", &uri, None, Some(&tok)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!cancelled.is_empty());

    // A design that is not a quotation design is refused, not silently used.
    let (status, _, _) = request(
        &ctx.router,
        "GET",
        &format!("{uri}&template=doc_temp_vEf0Y7jQHQj2rIuO"),
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _, _) = request(
        &ctx.router,
        "GET",
        "/api/quotations/quo_missing/document?design=current",
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================================
// Sync bookkeeping (version / device) and upgrading an older database
// ============================================================================

#[tokio::test]
async fn cancel_bumps_the_sync_version_and_records_the_device() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let pool = sqlx::SqlitePool::connect(&ctx.db_url)
        .await
        .expect("the awaited step succeeds");
    let (version, device): (i64, Option<String>) =
        sqlx::query_as("SELECT version, updated_by_device FROM quotations WHERE key = ?")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("the awaited step succeeds");
    assert_eq!((version, device), (1, None));

    // The cancel carries the terminal's id, as the POS UI does with `X-Device-Id`.
    let response = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/quotations/{key}/cancel"))
                .header(AUTHORIZATION, format!("Bearer {tok}"))
                .header("x-device-id", "dev_counter_two")
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    assert_eq!(response.status(), StatusCode::OK);

    let (version, device, status): (i64, Option<String>, String) =
        sqlx::query_as("SELECT version, updated_by_device, status FROM quotations WHERE key = ?")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("the awaited step succeeds");
    assert_eq!(version, 2);
    assert_eq!(device.as_deref(), Some("dev_counter_two"));
    assert_eq!(status, "cancelled");
}

#[tokio::test]
async fn a_database_from_before_sync_gains_the_sync_columns_and_keeps_its_quotations() {
    let path = std::env::temp_dir().join(format!("pos_quo_old_{}.db", Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());

    // An older install: the table exists in its old shape, with one quotation in it.
    {
        let old = sqlx::SqlitePool::connect(&url)
            .await
            .expect("the awaited step succeeds");
        sqlx::query(OLD_QUOTATIONS_TABLE)
            .execute(&old)
            .await
            .expect("the awaited step succeeds");
        sqlx::query(
            "INSERT INTO quotations VALUES ('quo_old', '64b0000000000000000000c1', 'QUO-0001', \
             'active', NULL, 'Nimal', NULL, NULL, '[]', 1000, 'fixed', 0.0, 0, 1000, '2030-01-01', \
             NULL, NULL, NULL, '{}', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
        )
        .execute(&old)
        .await
        .expect("the awaited step succeeds");
        old.close().await;
    }

    // Opening it (as every start-up does) must add the columns before the capture
    // triggers that read `NEW.version` are installed.
    let pool = clients::sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("upgrade succeeds");

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('quotations')")
            .fetch_all(&pool)
            .await
            .expect("the awaited step succeeds");
    for added in ["version", "deleted_at", "updated_by_device"] {
        assert!(
            columns.iter().any(|c| c == added),
            "missing {added}: {columns:?}"
        );
    }
    let (version, number): (i64, String) =
        sqlx::query_as("SELECT version, quotation_number FROM quotations WHERE key = 'quo_old'")
            .fetch_one(&pool)
            .await
            .expect("the awaited step succeeds");
    assert_eq!((version, number.as_str()), (1, "QUO-0001"));

    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync_quotations_%'",
    )
    .fetch_all(&pool)
    .await
    .expect("the awaited step succeeds");
    assert!(!triggers.is_empty(), "capture triggers were not installed");
    let index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_quotations_sync'",
    )
    .fetch_one(&pool)
    .await
    .expect("the awaited step succeeds");
    assert_eq!(index, 1);

    // And the table is still writable through the triggers.
    sqlx::query(
        "UPDATE quotations SET status = 'cancelled', version = version + 1 WHERE key = 'quo_old'",
    )
    .execute(&pool)
    .await
    .expect("a write fires the capture triggers without error");

    // Running the upgrade again changes nothing.
    pool.close().await;
    let again = clients::sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("idempotent");
    let version: i64 = sqlx::query_scalar("SELECT version FROM quotations WHERE key = 'quo_old'")
        .fetch_one(&again)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(version, 2);
    again.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
}

#[tokio::test]
async fn out_of_stock_quotation_creation_replenishment_and_fulfillment() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);

    // 1. Create Category and Subcategory
    let (_, cat) = call(
        &ctx.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Audio", "icon": "headphones", "color": "purple" })),
        &tok,
    )
    .await;
    let category_key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (_, sub) = call(
        &ctx.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Earbuds" })),
        &tok,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // 2. Create product with initial opening stock of 1 @ 15,000 cents
    let (status, product) = call(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Wireless Buds Pro",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "minStockThreshold": 2,
            "openingStock": { "quantity": 1, "unitCostCents": 10000, "sellingPriceCents": 15000 }
        })),
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let product_key = product["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product_id = product["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // 3. Sell that 1 item so stock drops to 0
    let (status, sale1) = call(
        &ctx.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Alice" },
            "items": [{
                "productKey": product_key,
                "quantity": 1,
                "sellingPriceCents": 15000,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 15000 },
            "shopProfileSnapshot": { "tradingName": "TechFix Repairs" }
        })),
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sale1}");

    // Verify product is out of stock (stock_quantity = 0), and has lastSellingPriceCents = 15000
    let (status, prod_res) = call(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{prod_res}");
    assert_eq!(prod_res["data"]["stockQuantity"], 0);
    assert_eq!(prod_res["data"]["lastSellingPriceCents"], 15000);

    // 4. Create a quotation quoting 2 units of the out-of-stock product
    let (status, quo_res) = create(
        &ctx.router,
        &tok,
        json!({
            "customer": { "customerName": "Kamal Gunaratne", "customerPhone": "0779998888" },
            "items": [{
                "productKey": product_key,
                "quantity": 2,
                "sellingPriceCents": 15000
            }],
            "validUntil": date_in(7)
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{quo_res}");
    let quotation_key = quo_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(quo_res["data"]["status"], "active");
    assert_eq!(quo_res["data"]["hasOutOfStockItems"], true);

    // 5. Verify pulse report flags this quotation as pending stock with isReadyToFill = false
    let (status, pulse_res) = call(
        &ctx.router,
        "GET",
        "/api/reports/dashboard-pulse",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pulse_res}");
    let pending = pulse_res["data"]["pendingStockQuotations"]
        .as_array()
        .expect("pendingStockQuotations array");
    let found = pending.iter().find(|q| q["key"] == quotation_key);
    assert!(
        found.is_some(),
        "Quotation should be in pendingStockQuotations"
    );
    assert_eq!(found.expect("the step succeeds")["isReadyToFill"], false);

    // 6. Replenish stock via purchase receipt:
    let (_, supplier) = call(
        &ctx.router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": "Audio World",
            "contactPerson": "Sam",
            "primaryPhone": "0771234567",
            "address": "Colombo",
            "suppliedCategories": [category_key]
        })),
        &tok,
    )
    .await;
    let supplier_key = supplier["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, rcp_res) = call(
        &ctx.router,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": "2026-03-01T00:00:00Z",
            "lines": [{
                "productKey": product_key,
                "quantity": 5,
                "lineTotalCents": 50000,
                "sellingPriceCents": 15000
            }],
            "confirmDuplicate": false
        })),
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{rcp_res}");

    // 7. Pulse now shows isReadyToFill = true!
    let (status, pulse_res2) = call(
        &ctx.router,
        "GET",
        "/api/reports/dashboard-pulse",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pulse_res2}");
    let pending2 = pulse_res2["data"]["pendingStockQuotations"]
        .as_array()
        .expect("pendingStockQuotations array");
    let found2 = pending2
        .iter()
        .find(|q| q["key"] == quotation_key)
        .expect("a match exists");
    assert_eq!(found2["isReadyToFill"], true);

    // 8. Fulfill the quotation by completing a sale with quotationKey
    let (status, sale2) = call(
        &ctx.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Alice" },
            "customer": { "customerName": "Kamal Gunaratne", "customerPhone": "0779998888" },
            "items": [{
                "productKey": product_key,
                "quantity": 2,
                "sellingPriceCents": 15000,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 30000 },
            "quotationKey": quotation_key,
            "shopProfileSnapshot": { "tradingName": "TechFix Repairs" }
        })),
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sale2}");
    let invoice_key = sale2["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // 9. Verify quotation is now fulfilled
    let (status, quo_after) = call(
        &ctx.router,
        "GET",
        &format!("/api/quotations/{quotation_key}"),
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{quo_after}");
    assert_eq!(quo_after["data"]["status"], "fulfilled");
    assert_eq!(quo_after["data"]["fulfilledByInvoiceKey"], invoice_key);
    assert!(quo_after["data"]["fulfilledAt"].as_str().is_some());

    // 10. Pulse no longer lists this quotation because it is fulfilled!
    let (status, pulse_res3) = call(
        &ctx.router,
        "GET",
        "/api/reports/dashboard-pulse",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pulse_res3}");
    let pending3 = pulse_res3["data"]["pendingStockQuotations"]
        .as_array()
        .expect("pendingStockQuotations array");
    assert!(!pending3.iter().any(|q| q["key"] == quotation_key));
}

// ============================================================================
// Mongo smoke test (real Atlas via MONGODB_URI, like the other spawn_app tests)
// ============================================================================

#[tokio::test]
async fn mongo_create_list_cancel_and_print() {
    let app = common::spawn_app().await;
    let manager = token(&app.config, Role::Manager);
    let staff = token(&app.config, Role::Staff);

    let (status, created) = create(&app.router, &manager, valid_body()).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let q = &created["data"];
    assert_eq!(q["subtotalCents"], 22000);
    assert_eq!(q["totalCents"], 19800);
    assert!(
        q["quotationNumber"]
            .as_str()
            .expect("the value is a string")
            .starts_with("QUO-")
    );
    let key = q["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, _) = call(&app.router, "GET", "/api/quotations", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, list) = call(
        &app.router,
        "GET",
        "/api/quotations?search=Nimal&status=active",
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(
        list["data"]["quotations"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .any(|x| x["key"] == key)
    );

    let (status, _, pdf) = request(
        &app.router,
        "GET",
        &format!("/api/quotations/{key}/document?design=current"),
        None,
        Some(&manager),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!pdf.is_empty());

    let cancel_uri = format!("/api/quotations/{key}/cancel");
    let (status, res) = call(&app.router, "POST", &cancel_uri, None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "cancelled");
    let (status, _) = call(&app.router, "POST", &cancel_uri, None, &manager).await;
    assert_eq!(status, StatusCode::CONFLICT);
}
