// Two-tenant route walk (Workstream A.7): with the real router in
// `TENANT_MODE=multi` against a throwaway Atlas database, tenant A is seeded
// through the real API, then EVERY GET operation of the generated OpenAPI
// document is called as tenant A (proving the walk is not vacuous) and as
// tenant B (proving nothing of A's is visible). Cross-tenant mutations and a
// token without a tenant are checked too, and afterwards no document may be
// unstamped or quarantined under `__deny__`.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;
use uuid::Uuid;

// The four tests each spin up a throwaway database and run heavy analytics
// pipelines; run concurrently they exhaust the shared Atlas tier and time out
// (500s after ~40s). One at a time is fast enough (about 95s) and reliable.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Routes that currently misbehave for tenant B, recorded explicitly so the
// failure list is visible instead of the walk being loosened.
// Format: (route template, why). Fill in only after confirming a real defect.
const KNOWN_ISSUES: &[(&str, &str)] = &[];

// By-id / by-key reads of something tenant A owns: tenant B must get 404.
const STRICT_404: &[&str] = &[
    "/api/inventory/products/{id}",
    "/api/inventory/products/by-barcode/{barcode}",
    "/api/billing/invoices/{id}",
    "/api/customers/{id}",
    "/api/suppliers/{id}",
    "/api/employees/{id}",
];

// List endpoints where tenant A must see its own seeded data and tenant B an
// empty list.
const LIST_ROUTES: &[&str] = &[
    "/api/inventory/products",
    "/api/inventory/categories",
    "/api/customers",
    "/api/suppliers",
    "/api/employees",
    "/api/billing/invoices",
];

struct Seed {
    category_key: String,
    product_id: String,
    product_key: String,
    barcode: String,
    customer_id: String,
    supplier_id: String,
    employee_id: String,
    invoice_id: String,
    /// Unique strings that exist only in tenant A's data (ids, keys, names,
    /// numbers). Any of them in a tenant-B response is a leak.
    needles: Vec<String>,
}

async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {t}"));
    }
    let request = builder
        .body(match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn json_of(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

fn text(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing {pointer} in {v}"))
        .to_string()
}

fn admin_token(app: &common::TestApp, tenant: Option<&str>) -> String {
    common::mint_token_for_tenant(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
        tenant,
    )
}

async fn seed_tenant_a(app: &common::TestApp, token: &str) -> Seed {
    let uniq = Uuid::new_v4().simple().to_string();
    let mut needles = Vec::new();

    let category_name = format!("CatA-{uniq}");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(token),
        Some(json!({ "name": category_name, "icon": "Box", "color": "blue", "subcategories": ["Alpha"] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "category: {body}");
    let v = json_of(&body);
    let category_key = text(&v, "/data/key");
    let subcategory_key = text(&v, "/data/subcategories/0/key");
    needles.extend([category_name, category_key.clone(), subcategory_key.clone()]);

    let product_name = format!("ProdA-{uniq}");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(token),
        Some(json!({
            "name": product_name,
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 10,
            "minStockThreshold": 3,
            "autoGenerateBarcode": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "product: {body}");
    let v = json_of(&body);
    let (product_id, product_key) = (text(&v, "/data/id"), text(&v, "/data/key"));
    let (barcode, sku) = (text(&v, "/data/barcode"), text(&v, "/data/sku"));
    needles.extend([
        product_name,
        product_id.clone(),
        product_key.clone(),
        barcode.clone(),
        sku,
    ]);

    let customer_name = format!("CustA-{uniq}");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/customers",
        Some(token),
        Some(json!({ "name": customer_name, "primaryPhone": format!("077{:07}", Uuid::new_v4().as_u128() % 10_000_000) })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "customer: {body}");
    let v = json_of(&body);
    let customer_id = text(&v, "/data/id");
    needles.extend([customer_name, customer_id.clone(), text(&v, "/data/key")]);

    let supplier_name = format!("SuppA-{uniq}");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(token),
        Some(json!({
            "name": supplier_name,
            "contactPerson": "Test Contact",
            "primaryPhone": "077 123 4567",
            "address": "123 Test Street",
            "suppliedCategories": ["Phone Parts"],
            "email": "supplier@example.com",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "supplier: {body}");
    let v = json_of(&body);
    let supplier_id = text(&v, "/data/id");
    needles.extend([supplier_name, supplier_id.clone(), text(&v, "/data/key")]);

    let employee_name = format!("EmpA-{uniq}");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/employees",
        Some(token),
        Some(json!({
            "name": employee_name,
            "phone": "0771234567",
            "role": "technician",
            "defaultSplitType": "percentage",
            "defaultSplitValue": 20.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "employee: {body}");
    let v = json_of(&body);
    let employee_id = text(&v, "/data/id");
    needles.extend([employee_name, employee_id.clone(), text(&v, "/data/key")]);

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(token),
        Some(json!({
            "staff": { "cashierName": format!("CashierA-{uniq}") },
            "items": [{ "productKey": product_key, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 2000 },
            "shopProfileSnapshot": { "tradingName": format!("ShopA-{uniq}") },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sale: {body}");
    let v = json_of(&body);
    let invoice_id = text(&v, "/data/invoice/id");
    needles.extend([
        invoice_id.clone(),
        text(&v, "/data/invoice/key"),
        text(&v, "/data/invoice/invoiceNumber"),
        format!("CashierA-{uniq}"),
        format!("ShopA-{uniq}"),
    ]);

    Seed {
        category_key,
        product_id,
        product_key,
        barcode,
        customer_id,
        supplier_id,
        employee_id,
        invoice_id,
        needles,
    }
}

async fn get_routes(router: &axum::Router) -> Vec<String> {
    let (status, body) = call(router, "GET", "/api-docs/openapi.json", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let spec = json_of(&body);
    let mut routes: Vec<String> = spec["paths"]
        .as_object()
        .expect("paths")
        .iter()
        .filter(|(_, item)| item.get("get").is_some())
        .map(|(path, _)| path.clone())
        .filter(|p| p != "/api/health")
        .collect();
    routes.sort();
    assert!(
        routes.len() > 20,
        "expected the full GET surface, got {routes:?}"
    );
    routes
}

/// Fills a path template with values that exist only in tenant A.
fn resolve(template: &str, s: &Seed) -> String {
    let id = if template.starts_with("/api/inventory/products") {
        s.product_id.clone()
    } else if template.starts_with("/api/billing/invoices") {
        s.invoice_id.clone()
    } else if template.starts_with("/api/customers") {
        s.customer_id.clone()
    } else if template.starts_with("/api/suppliers") {
        s.supplier_id.clone()
    } else if template.starts_with("/api/employees") {
        s.employee_id.clone()
    } else {
        "000000000000000000000000".to_string()
    };
    template
        .replace("{categoryKey}", &s.category_key)
        .replace("{barcode}", &s.barcode)
        .replace("{documentType}", "invoice")
        .replace("{key}", &s.product_key)
        .replace("{id}", &id)
}

fn leaks<'a>(body: &str, needles: &'a [String]) -> Vec<&'a String> {
    needles
        .iter()
        .filter(|n| body.contains(n.as_str()))
        .collect()
}

fn known_issue(template: &str) -> bool {
    KNOWN_ISSUES.iter().any(|(t, _)| *t == template)
}

/// First array found directly under `data` (the list payload shape used by
/// every list endpoint), if any.
fn first_list(body: &Value) -> Option<&Vec<Value>> {
    body.get("data")?
        .as_object()?
        .values()
        .find_map(Value::as_array)
}

#[tokio::test]
async fn tenant_b_sees_nothing_of_tenant_a_on_any_get_route() {
    let _serial = SERIAL.lock().await;
    dotenvy::dotenv().ok();
    if std::env::var("MONGODB_URI").is_err() {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    }
    let app = common::spawn_app_multi_tenant().await;
    let token_a = admin_token(&app, Some("tenant_a"));
    let token_b = admin_token(&app, Some("tenant_b"));

    let seed = seed_tenant_a(&app, &token_a).await;
    let routes = get_routes(&app.router).await;

    for strict in STRICT_404.iter().chain(LIST_ROUTES) {
        assert!(
            routes.iter().any(|r| r == strict),
            "route template {strict} is not in the OpenAPI document - update the constants"
        );
    }

    // --- Tenant A: not vacuous ------------------------------------------------
    let mut a_hits = 0;
    for template in &routes {
        let uri = format!("{}?page=1&limit=100", resolve(template, &seed));
        let (status, body) = call(&app.router, "GET", &uri, Some(&token_a), None).await;
        if !leaks(&body, &seed.needles).is_empty() {
            a_hits += 1;
        }
        if STRICT_404.contains(&template.as_str()) || LIST_ROUTES.contains(&template.as_str()) {
            assert_eq!(status, StatusCode::OK, "tenant A GET {uri}: {body}");
            assert!(
                !leaks(&body, &seed.needles).is_empty(),
                "tenant A GET {uri} does not return its own data - walk would be vacuous: {body}"
            );
        }
    }
    assert!(a_hits >= 8, "only {a_hits} routes returned tenant A data");

    // --- Tenant B: nothing leaks ---------------------------------------------
    let mut failures: Vec<String> = Vec::new();
    for template in &routes {
        let uri = format!("{}?page=1&limit=100", resolve(template, &seed));
        let (status, body) = call(&app.router, "GET", &uri, Some(&token_b), None).await;
        let mut problems = Vec::new();
        let found = leaks(&body, &seed.needles);
        if !found.is_empty() {
            problems.push(format!("leaked {found:?}"));
        }
        if status.is_server_error() {
            // Some report/document routes intermittently time out on the shared
            // Atlas tier (~20-40s, then 500) for ANY tenant. Only a 5xx that
            // tenant A does not also get on the same request counts as a
            // tenant-B defect.
            let (a_status, _) = call(&app.router, "GET", &uri, Some(&token_a), None).await;
            if !a_status.is_server_error() {
                problems.push(format!("server error {status} only for tenant B: {body}"));
            }
        }
        if STRICT_404.contains(&template.as_str()) && status != StatusCode::NOT_FOUND {
            problems.push(format!("expected 404, got {status}"));
        }
        if LIST_ROUTES.contains(&template.as_str()) {
            if status != StatusCode::OK {
                problems.push(format!("list expected 200, got {status}: {body}"));
            } else if first_list(&json_of(&body)).is_none_or(|l| !l.is_empty()) {
                problems.push(format!("list not empty: {body}"));
            }
        }
        if !problems.is_empty() && !known_issue(template) {
            failures.push(format!("GET {template}: {}", problems.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "tenant B isolation failures:\n{}",
        failures.join("\n")
    );

    app.db.drop().await.ok();
}

#[tokio::test]
async fn cross_tenant_mutations_fail_with_404_and_leave_data_intact() {
    let _serial = SERIAL.lock().await;
    dotenvy::dotenv().ok();
    if std::env::var("MONGODB_URI").is_err() {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    }
    let app = common::spawn_app_multi_tenant().await;
    let token_a = admin_token(&app, Some("tenant_a"));
    let token_b = admin_token(&app, Some("tenant_b"));
    let seed = seed_tenant_a(&app, &token_a).await;

    let product = format!("/api/inventory/products/{}", seed.product_id);
    let attempts: Vec<(&str, String, Option<Value>)> = vec![
        ("PUT", product.clone(), Some(json!({ "name": "HACKED" }))),
        (
            "PATCH",
            format!("{product}/stock"),
            Some(json!({ "delta": -5, "reason": "hack" })),
        ),
        ("DELETE", product.clone(), None),
        (
            "PATCH",
            format!("/api/customers/{}", seed.customer_id),
            Some(json!({ "name": "HACKED" })),
        ),
        (
            "DELETE",
            format!("/api/customers/{}", seed.customer_id),
            None,
        ),
        (
            "DELETE",
            format!("/api/suppliers/{}", seed.supplier_id),
            None,
        ),
        (
            "PATCH",
            format!("/api/employees/{}", seed.employee_id),
            Some(json!({ "name": "HACKED" })),
        ),
        (
            "DELETE",
            format!("/api/employees/{}", seed.employee_id),
            None,
        ),
        (
            "DELETE",
            format!("/api/inventory/categories/{}", seed.category_key),
            None,
        ),
        (
            "POST",
            format!("/api/billing/invoices/{}/cancel", seed.invoice_id),
            Some(json!({})),
        ),
    ];
    let mut failures = Vec::new();
    for (method, uri, body) in attempts {
        let (status, resp) = call(&app.router, method, &uri, Some(&token_b), body).await;
        if status != StatusCode::NOT_FOUND {
            failures.push(format!("{method} {uri} as tenant B -> {status}: {resp}"));
        }
    }
    assert!(
        failures.is_empty(),
        "cross-tenant mutations not rejected:\n{}",
        failures.join("\n")
    );

    // Tenant A's data is untouched.
    let (status, body) = call(&app.router, "GET", &product, Some(&token_a), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v = json_of(&body);
    assert!(v["data"]["name"].as_str().unwrap().starts_with("ProdA-"));
    assert_eq!(v["data"]["stockQuantity"], 9, "stock changed: 10 - 1 sold");
    for uri in [
        format!("/api/customers/{}", seed.customer_id),
        format!("/api/suppliers/{}", seed.supplier_id),
        format!("/api/employees/{}", seed.employee_id),
        format!("/api/billing/invoices/{}", seed.invoice_id),
    ] {
        let (status, body) = call(&app.router, "GET", &uri, Some(&token_a), None).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert!(!body.contains("HACKED"), "{uri} was modified: {body}");
    }
    let (_, body) = call(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{}", seed.invoice_id),
        Some(&token_a),
        None,
    )
    .await;
    assert_ne!(json_of(&body)["data"]["status"], "cancelled");

    app.db.drop().await.ok();
}

#[tokio::test]
async fn token_without_tenant_is_rejected_and_no_document_is_unstamped() {
    let _serial = SERIAL.lock().await;
    dotenvy::dotenv().ok();
    if std::env::var("MONGODB_URI").is_err() {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    }
    let app = common::spawn_app_multi_tenant().await;
    let token_a = admin_token(&app, Some("tenant_a"));
    let no_tenant = admin_token(&app, None);
    seed_tenant_a(&app, &token_a).await;

    let (status, body) = call(
        &app.router,
        "GET",
        "/api/inventory/products",
        Some(&no_tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    let (status, _) = call(&app.router, "GET", "/api/inventory/products", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Every document written while seeding carries a real tenant: none
    // unstamped, none quarantined under the deny scope.
    let mut problems = Vec::new();
    for name in app.db.list_collection_names().await.unwrap() {
        let coll = app.db.collection::<Document>(&name);
        let unstamped = coll
            .count_documents(doc! { "tenant_id": { "$exists": false } })
            .await
            .unwrap();
        let denied = coll
            .count_documents(doc! { "tenant_id": "__deny__" })
            .await
            .unwrap();
        if unstamped > 0 || denied > 0 {
            problems.push(format!(
                "{name}: {unstamped} unstamped, {denied} quarantined"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "documents without a real tenant:\n{}",
        problems.join("\n")
    );

    app.db.drop().await.ok();
}

// Aggregates (stats, dashboards, the cached reports engine) hold no needle
// strings, so compare the numbers directly: tenant A is queried first so a
// tenant-blind cache entry would be served to tenant B.
#[tokio::test]
async fn aggregates_and_cached_reports_are_tenant_scoped() {
    let _serial = SERIAL.lock().await;
    dotenvy::dotenv().ok();
    if std::env::var("MONGODB_URI").is_err() {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    }
    let app = common::spawn_app_multi_tenant().await;
    let token_a = admin_token(&app, Some("tenant_a"));
    let token_b = admin_token(&app, Some("tenant_b"));
    seed_tenant_a(&app, &token_a).await;

    // (route, JSON pointer to a counter, tenant A's expected value)
    let checks = [
        ("/api/inventory/stats", "/data/totalItems", 1),
        (
            "/api/inventory/overview?page=1&limit=10",
            "/data/metrics/totalItems",
            1,
        ),
        ("/api/billing/invoices/stats", "/data/todayInvoiceCount", 1),
        ("/api/billing/invoices/stats", "/data/todaySalesCents", 2000),
        ("/api/customers/stats", "/data/totalCustomers", 1),
        (
            "/api/reports/dashboard?preset=all_time",
            "/data/totalRevenueCents",
            2000,
        ),
        (
            "/api/reports/engine/feed?section=overview&preset=all_time",
            "/data/overview/summary/current/totalRevenueCents",
            2000,
        ),
    ];
    let mut failures = Vec::new();
    for (uri, pointer, expected_a) in checks {
        let (status, body) = call(&app.router, "GET", uri, Some(&token_a), None).await;
        let got_a = json_of(&body).pointer(pointer).and_then(Value::as_i64);
        if status != StatusCode::OK || got_a != Some(expected_a) {
            failures.push(format!(
                "A {uri} {pointer}: expected {expected_a}, got {got_a:?} ({status})"
            ));
        }
    }
    for (uri, pointer, _) in checks {
        let (status, body) = call(&app.router, "GET", uri, Some(&token_b), None).await;
        let got_b = json_of(&body).pointer(pointer).and_then(Value::as_i64);
        if status != StatusCode::OK || got_b != Some(0) {
            failures.push(format!(
                "B {uri} {pointer}: expected 0, got {got_b:?} ({status})"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "aggregate isolation failures:\n{}",
        failures.join("\n")
    );

    app.db.drop().await.ok();
}
