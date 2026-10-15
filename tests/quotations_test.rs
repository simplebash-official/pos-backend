// Quotations: Admin/Manager-only access, server-side totals, validation,
// numbering, list filters, cancel, PDF printing, and "no side effects on
// stock or invoices". The behaviour scenarios run against in-memory-style
// SQLite (always); a smaller smoke test runs the same helpers against the
// real Mongo `spawn_app()` so the Mongo repository branch is exercised too.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    app::{self, AppState},
    clients::{self, db::Db},
    core::{
        config::{Config, DatabaseType},
        constants::{permissions as perm, roles},
    },
    domain::users::Role,
};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ============================================================================
// SQLite harness (same construction as `sqlite_integration_test.rs`)
// ============================================================================

struct Ctx {
    router: axum::Router,
    config: Arc<Config>,
    db_url: String,
    db_path: std::path::PathBuf,
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(self.db_path.with_extension("db-shm"));
        let _ = std::fs::remove_file(self.db_path.with_extension("db-wal"));
    }
}

async fn setup() -> Ctx {
    dotenvy::dotenv().ok();
    let db_path = std::env::temp_dir().join(format!("pos_quo_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = clients::sqlite::connect(&db_url)
        .await
        .expect("failed to initialize SQLite test database");

    let config = Arc::new(Config {
        database_type: DatabaseType::Sqlite,
        database_url: db_url.clone(),
        mongodb_uri: String::new(),
        mongodb_db_name: String::new(),
        jwt_secret: "test-sqlite-jwt-secret-key-that-is-long-enough".to_string(),
        port: 8080,
        bind_addr: "127.0.0.1".to_string(),
        jwt_expiry_hours: 12,
        document_server_url: common::start_mock_document_server().await,
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
    });
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db = Db::Sqlite(pool);
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()),
    );
    let router = app::build_router(AppState {
        config: config.clone(),
        db,
        document_server,
        reports_engine,
    });
    Ctx {
        router,
        config,
        db_url,
        db_path,
    }
}

// ============================================================================
// Request helpers (shared by the SQLite and Mongo runs)
// ============================================================================

async fn request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let (status, _, bytes) = request(router, method, uri, body, Some(token)).await;
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn token(config: &Config, role: Role) -> String {
    common::mint_token(config, Some(role), roles::default_permissions(role))
}

fn date_in(days: i64) -> String {
    (Utc::now() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

/// A valid request: two ad-hoc lines (22,000 gross, one line discount) and a
/// 10% order discount, so every total below is checkable by hand.
fn valid_body() -> Value {
    json!({
        "customer": { "customerName": "Nimal Perera", "customerPhone": "0771234567",
                      "customerAddress": "12 Temple Road, Kandy" },
        "items": [
            { "name": "Screen replacement", "unitPriceCents": 10000, "quantity": 2, "discountCents": 500 },
            { "name": "Tempered glass", "unitPriceCents": 2500, "quantity": 1 }
        ],
        "pricingAdjustments": { "discountType": "percentage", "discountValue": 10.0 },
        "validUntil": date_in(14),
        "notes": "Price excludes courier.",
        "terms": "50% advance to start work.",
        "preparedBy": "Kamal"
    })
}

async fn create(router: &axum::Router, tok: &str, body: Value) -> (StatusCode, Value) {
    call(router, "POST", "/api/quotations", Some(body), tok).await
}

// ============================================================================
// Access control
// ============================================================================

#[tokio::test]
async fn only_admin_and_manager_can_use_quotations() {
    let ctx = setup().await;
    let admin = token(&ctx.config, Role::Admin);
    let manager = token(&ctx.config, Role::Manager);
    let staff = token(&ctx.config, Role::Staff);
    // A token that has a role but none of the permissions.
    let bare = common::mint_token(&ctx.config, Some(Role::Manager), &[]);

    // Admin and Manager can create.
    for tok in [&admin, &manager] {
        let (status, body) = create(&ctx.router, tok, valid_body()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (_, created) = create(&ctx.router, &admin, valid_body()).await;
    let key = created["data"]["key"].as_str().unwrap().to_string();

    // Staff and permission-less tokens are refused on every route.
    for tok in [&staff, &bare] {
        for (method, uri, body) in [
            ("GET", "/api/quotations".to_string(), None),
            ("POST", "/api/quotations".to_string(), Some(valid_body())),
            ("GET", format!("/api/quotations/{key}"), None),
            ("POST", format!("/api/quotations/{key}/cancel"), None),
            ("GET", format!("/api/quotations/{key}/document"), None),
        ] {
            let (status, _, bytes) = request(&ctx.router, method, &uri, body, Some(tok)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
            let json: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["code"], "PERMISSION_DENIED");
        }
    }

    // And a missing token is a 401, not a 403.
    let (status, _, _) = request(&ctx.router, "GET", "/api/quotations", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The permission itself is held by exactly Admin and Manager.
    assert!(roles::default_permissions(Role::Admin).contains(&perm::QUOTATIONS_MANAGE));
    assert!(roles::default_permissions(Role::Manager).contains(&perm::QUOTATIONS_MANAGE));
    assert!(!roles::default_permissions(Role::Staff).contains(&perm::QUOTATIONS_MANAGE));

    // Staff really saw nothing created on their behalf.
    let (_, list) = call(&ctx.router, "GET", "/api/quotations", None, &admin).await;
    assert_eq!(list["data"]["total"], 3);
}

// ============================================================================
// Create: totals, numbering, validation
// ============================================================================

#[tokio::test]
async fn totals_are_computed_by_the_server_and_numbers_increment() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);

    // Client-supplied totals must be ignored.
    let mut body = valid_body();
    body["subtotalCents"] = json!(1);
    body["totalCents"] = json!(1);
    let (status, first) = create(&ctx.router, &tok, body).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let q = &first["data"];

    // Lines: 10000*2-500 = 19500 and 2500 -> subtotal 22000; 10% -> 2200.
    assert_eq!(q["items"][0]["totalCents"], 19500);
    assert_eq!(q["items"][1]["totalCents"], 2500);
    assert_eq!(q["subtotalCents"], 22000);
    assert_eq!(q["discountType"], "percentage");
    assert_eq!(q["discountCents"], 2200);
    assert_eq!(q["totalCents"], 19800);
    assert_eq!(q["status"], "active");
    assert_eq!(q["isExpired"], false);
    assert_eq!(q["customerName"], "Nimal Perera");
    assert_eq!(q["preparedBy"], "Kamal");
    assert!(q["key"].as_str().unwrap().starts_with("quo_"));
    // The frozen shop profile is not leaked through the API.
    assert!(q.get("shopProfileSnapshot").is_none());

    assert_eq!(q["quotationNumber"], "QUO-0001");
    let (_, second) = create(&ctx.router, &tok, valid_body()).await;
    assert_eq!(second["data"]["quotationNumber"], "QUO-0002");

    // A fixed order discount, and no discount at all.
    let mut fixed = valid_body();
    fixed["pricingAdjustments"] = json!({ "discountType": "fixed", "discountValue": 1000.0 });
    let (_, fixed_res) = create(&ctx.router, &tok, fixed).await;
    assert_eq!(fixed_res["data"]["discountCents"], 1000);
    assert_eq!(fixed_res["data"]["totalCents"], 21000);

    let mut none = valid_body();
    none.as_object_mut().unwrap().remove("pricingAdjustments");
    let (_, none_res) = create(&ctx.router, &tok, none).await;
    assert_eq!(none_res["data"]["discountCents"], 0);
    assert_eq!(none_res["data"]["totalCents"], 22000);
}

#[tokio::test]
async fn invalid_requests_are_rejected_and_do_not_burn_a_number() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);

    // (label, how to break an otherwise valid request)
    type Case = (&'static str, Box<dyn Fn(&mut Value)>);
    let cases: Vec<Case> = vec![
        ("no items", Box::new(|b| b["items"] = json!([]))),
        (
            "zero quantity",
            Box::new(|b| b["items"][0]["quantity"] = json!(0)),
        ),
        (
            "negative price",
            Box::new(|b| b["items"][0]["unitPriceCents"] = json!(-1)),
        ),
        (
            "missing price",
            Box::new(|b| {
                b["items"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("unitPriceCents");
            }),
        ),
        (
            "blank item name",
            Box::new(|b| b["items"][0]["name"] = json!("   ")),
        ),
        (
            "line discount above price",
            Box::new(|b| b["items"][0]["discountCents"] = json!(20001)),
        ),
        (
            "negative line discount",
            Box::new(|b| b["items"][0]["discountCents"] = json!(-5)),
        ),
        (
            "past valid-until",
            Box::new(|b| b["validUntil"] = json!(date_in(-1))),
        ),
        (
            "malformed valid-until",
            Box::new(|b| b["validUntil"] = json!("31/12/2030")),
        ),
        (
            "percentage over 100",
            Box::new(|b| {
                b["pricingAdjustments"] =
                    json!({ "discountType": "percentage", "discountValue": 101.0 })
            }),
        ),
        (
            "unknown discount type",
            Box::new(|b| {
                b["pricingAdjustments"] = json!({ "discountType": "bogo", "discountValue": 1.0 })
            }),
        ),
        (
            "negative order discount",
            Box::new(|b| {
                b["pricingAdjustments"] = json!({ "discountType": "fixed", "discountValue": -1.0 })
            }),
        ),
        (
            "no customer name",
            Box::new(|b| b["customer"] = json!({ "customerPhone": "0771234567" })),
        ),
        (
            "overlong notes",
            Box::new(|b| b["notes"] = json!("x".repeat(2001))),
        ),
    ];
    for (label, mutate) in cases {
        let mut body = valid_body();
        mutate(&mut body);
        let (status, res) = create(&ctx.router, &tok, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: {res}");
    }

    // Unknown references are 404s, also before any number is reserved.
    let mut bad_product = valid_body();
    bad_product["items"][0] = json!({ "productKey": "prod_nope", "quantity": 1 });
    assert_eq!(
        create(&ctx.router, &tok, bad_product).await.0,
        StatusCode::NOT_FOUND
    );
    let mut bad_customer = valid_body();
    bad_customer["customer"] = json!({ "customerKey": "cust_nope" });
    assert_eq!(
        create(&ctx.router, &tok, bad_customer).await.0,
        StatusCode::NOT_FOUND
    );

    // Fifteen rejected requests later, the first real quotation is still #1.
    let (status, ok) = create(&ctx.router, &tok, valid_body()).await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    assert_eq!(ok["data"]["quotationNumber"], "QUO-0001");

    // Valid-until of today is allowed (the last day the offer holds).
    let mut today = valid_body();
    today["validUntil"] = json!(date_in(0));
    assert_eq!(create(&ctx.router, &tok, today).await.0, StatusCode::OK);
}

#[tokio::test]
async fn catalog_lines_use_the_catalog_and_do_not_touch_stock_or_invoices() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);

    let (_, cat) = call(
        &ctx.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Phones", "icon": "devices", "color": "blue" })),
        &tok,
    )
    .await;
    let category_key = cat["data"]["key"].as_str().unwrap().to_string();
    let (_, sub) = call(
        &ctx.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Smart" })),
        &tok,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, product) = call(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Galaxy A15", "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "minStockThreshold": 2,
            "openingStock": { "quantity": 10, "unitCostCents": 50000, "sellingPriceCents": 65000 }
        })),
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let product_key = product["data"]["key"].as_str().unwrap().to_string();
    let product_id = product["data"]["id"].as_str().unwrap().to_string();
    let sku = product["data"]["sku"].as_str().unwrap().to_string();

    let (_, customer) = call(
        &ctx.router,
        "POST",
        "/api/customers",
        Some(
            json!({ "name": "Registered Rita", "primaryPhone": "0712223334",
                     "address": "5 Main St" }),
        ),
        &tok,
    )
    .await;
    let customer_key = customer["data"]["key"].as_str().unwrap().to_string();

    let (_, invoices_before) = call(&ctx.router, "GET", "/api/billing/invoices", None, &tok).await;

    // A product line must quote one of the product's current prices.
    for (line, code) in [
        (
            json!({ "productKey": product_key, "quantity": 1 }),
            "VALIDATION_ERROR",
        ),
        (
            json!({ "productKey": product_key, "sellingPriceCents": 70000, "quantity": 1 }),
            "PRICE_TIER_UNAVAILABLE",
        ),
    ] {
        let (status, res) = create(
            &ctx.router,
            &tok,
            json!({ "customer": { "customerName": "Walk-in" }, "items": [line], "validUntil": date_in(7) }),
        )
        .await;
        assert!(status.is_client_error(), "{res}");
        assert_eq!(res["code"], code, "{res}");
    }

    // The client lies about name and unit price; the catalog and the picked
    // price win. The registered customer's record wins over the typed name.
    let (status, res) = create(
        &ctx.router,
        &tok,
        json!({
            "customer": { "customerKey": customer_key, "customerName": "Somebody Else" },
            "items": [{ "productKey": product_key, "name": "Fake", "unitPriceCents": 1, "sellingPriceCents": 65000, "quantity": 3 }],
            "validUntil": date_in(7)
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let q = &res["data"];
    assert_eq!(q["items"][0]["name"], "Galaxy A15");
    assert_eq!(q["items"][0]["sku"], sku);
    assert_eq!(q["items"][0]["unitPriceCents"], 65000);
    assert_eq!(q["totalCents"], 195000);
    assert_eq!(q["customerName"], "Registered Rita");
    assert_eq!(q["customerPhone"], "0712223334");
    assert_eq!(q["customerAddress"], "5 Main St");
    assert_eq!(q["customerKey"], customer_key);

    // Stock is untouched and no invoice appeared.
    let (_, after) = call(
        &ctx.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &tok,
    )
    .await;
    assert_eq!(after["data"]["stockQuantity"], 10);
    let (_, invoices_after) = call(&ctx.router, "GET", "/api/billing/invoices", None, &tok).await;
    assert_eq!(
        invoices_before["data"]["total"],
        invoices_after["data"]["total"]
    );
}

// ============================================================================
// Read: get, list filters, expiry
// ============================================================================

#[tokio::test]
async fn list_search_status_filters_and_pagination() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);

    let mut for_amal = valid_body();
    for_amal["customer"]["customerName"] = json!("Amal Silva");
    let (_, amal) = create(&ctx.router, &tok, for_amal).await;
    let (_, nimal) = create(&ctx.router, &tok, valid_body()).await;
    let (_, expiring) = create(&ctx.router, &tok, valid_body()).await;

    // Cancel one; push another into the past (the API refuses to create one).
    let cancel_key = amal["data"]["key"].as_str().unwrap();
    call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{cancel_key}/cancel"),
        None,
        &tok,
    )
    .await;
    let pool = sqlx::SqlitePool::connect(&ctx.db_url).await.unwrap();
    sqlx::query("UPDATE quotations SET valid_until = '2000-01-01' WHERE key = ?")
        .bind(expiring["data"]["key"].as_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let list = |query: &'static str| {
        let router = ctx.router.clone();
        let tok = tok.clone();
        async move {
            call(
                &router,
                "GET",
                &format!("/api/quotations{query}"),
                None,
                &tok,
            )
            .await
        }
    };

    let (status, all) = list("").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["data"]["total"], 3);
    // Newest first.
    assert_eq!(all["data"]["quotations"][0]["key"], expiring["data"]["key"]);

    let (_, active) = list("?status=active").await;
    assert_eq!(active["data"]["total"], 1);
    assert_eq!(active["data"]["quotations"][0]["key"], nimal["data"]["key"]);

    let (_, expired) = list("?status=expired").await;
    assert_eq!(expired["data"]["total"], 1);
    assert_eq!(expired["data"]["quotations"][0]["isExpired"], true);
    assert_eq!(expired["data"]["quotations"][0]["status"], "active");

    let (_, cancelled) = list("?status=cancelled").await;
    assert_eq!(cancelled["data"]["total"], 1);
    assert_eq!(
        cancelled["data"]["quotations"][0]["customerName"],
        "Amal Silva"
    );
    // A cancelled quotation is never reported as expired.
    assert_eq!(cancelled["data"]["quotations"][0]["isExpired"], false);

    // Search by customer name, by number and by phone.
    assert_eq!(list("?search=amal").await.1["data"]["total"], 1);
    assert_eq!(list("?search=QUO-0002").await.1["data"]["total"], 1);
    // All three share the default phone number.
    assert_eq!(list("?search=0771234567").await.1["data"]["total"], 3);
    assert_eq!(list("?search=nobody-here").await.1["data"]["total"], 0);

    // Unknown status is a 400, not an empty list.
    assert_eq!(list("?status=bogus").await.0, StatusCode::BAD_REQUEST);

    // Pagination.
    let (_, page) = list("?limit=2&page=2").await;
    assert_eq!(page["data"]["total"], 3);
    assert_eq!(page["data"]["totalPages"], 2);
    assert_eq!(page["data"]["quotations"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_by_key_and_by_id_and_missing_is_404() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"].as_str().unwrap();
    let id = created["data"]["id"].as_str().unwrap();

    for handle in [key, id] {
        let (status, res) = call(
            &ctx.router,
            "GET",
            &format!("/api/quotations/{handle}"),
            None,
            &tok,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        assert_eq!(res["data"]["quotationNumber"], "QUO-0001");
    }

    let (status, res) = call(
        &ctx.router,
        "GET",
        "/api/quotations/quo_missing",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(res["code"], "QUOTATION_NOT_FOUND");
}

// ============================================================================
// Cancel + PDF
// ============================================================================

#[tokio::test]
async fn cancel_once_then_conflict() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Manager);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"].as_str().unwrap();

    let (status, res) = call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{key}/cancel"),
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "cancelled");

    let (status, res) = call(
        &ctx.router,
        "POST",
        &format!("/api/quotations/{key}/cancel"),
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(res["code"], "QUOTATION_ALREADY_CANCELLED");

    let (status, _) = call(
        &ctx.router,
        "POST",
        "/api/quotations/quo_missing/cancel",
        None,
        &tok,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn document_renders_a_pdf_with_the_quotation_template() {
    let ctx = setup().await;
    let tok = token(&ctx.config, Role::Admin);
    let (_, created) = create(&ctx.router, &tok, valid_body()).await;
    let key = created["data"]["key"].as_str().unwrap();
    let uri = format!("/api/quotations/{key}/document");

    let (status, headers, bytes) = request(&ctx.router, "GET", &uri, None, Some(&tok)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/pdf");
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
        &format!("{uri}?template=doc_temp_vEf0Y7jQHQj2rIuO"),
        None,
        Some(&tok),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _, _) = request(
        &ctx.router,
        "GET",
        "/api/quotations/quo_missing/document",
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
    let key = created["data"]["key"].as_str().unwrap().to_string();

    let pool = sqlx::SqlitePool::connect(&ctx.db_url).await.unwrap();
    let (version, device): (i64, Option<String>) =
        sqlx::query_as("SELECT version, updated_by_device FROM quotations WHERE key = ?")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
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
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (version, device, status): (i64, Option<String>, String) =
        sqlx::query_as("SELECT version, updated_by_device, status FROM quotations WHERE key = ?")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 2);
    assert_eq!(device.as_deref(), Some("dev_counter_two"));
    assert_eq!(status, "cancelled");
}

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
async fn a_database_from_before_sync_gains_the_sync_columns_and_keeps_its_quotations() {
    let path = std::env::temp_dir().join(format!("pos_quo_old_{}.db", Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());

    // An older install: the table exists in its old shape, with one quotation in it.
    {
        let old = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::query(OLD_QUOTATIONS_TABLE)
            .execute(&old)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO quotations VALUES ('quo_old', '64b0000000000000000000c1', 'QUO-0001', \
             'active', NULL, 'Nimal', NULL, NULL, '[]', 1000, 'fixed', 0.0, 0, 1000, '2030-01-01', \
             NULL, NULL, NULL, '{}', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
        )
        .execute(&old)
        .await
        .unwrap();
        old.close().await;
    }

    // Opening it (as every start-up does) must add the columns before the capture
    // triggers that read `NEW.version` are installed.
    let pool = clients::sqlite::connect(&url)
        .await
        .expect("upgrade succeeds");

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('quotations')")
            .fetch_all(&pool)
            .await
            .unwrap();
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
            .unwrap();
    assert_eq!((version, number.as_str()), (1, "QUO-0001"));

    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync_quotations_%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!triggers.is_empty(), "capture triggers were not installed");
    let index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_quotations_sync'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
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
    let again = clients::sqlite::connect(&url).await.expect("idempotent");
    let version: i64 = sqlx::query_scalar("SELECT version FROM quotations WHERE key = 'quo_old'")
        .fetch_one(&again)
        .await
        .unwrap();
    assert_eq!(version, 2);
    again.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
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
    assert!(q["quotationNumber"].as_str().unwrap().starts_with("QUO-"));
    let key = q["key"].as_str().unwrap().to_string();

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
            .unwrap()
            .iter()
            .any(|x| x["key"] == key)
    );

    let (status, _, pdf) = request(
        &app.router,
        "GET",
        &format!("/api/quotations/{key}/document"),
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
