//! Shared setup for the `inventory_test` tests.

use crate::common;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use mongodb::bson::DateTime as BsonDateTime;
use serde_json::Value;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::inventory::model::{CategoryDocument, SubcategoryDocument};
use tower::ServiceExt;
use uuid::Uuid;

pub(crate) async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the text is valid JSON")
    };
    (status, json)
}

/// The default request helper: authenticates as a Manager, which holds both
/// `inventory:read` and `inventory:write` and so can reach every non-admin
/// inventory route. Every route in this module except the module-status stub
/// requires a token, so a test that isn't *about* authorization should use
/// this rather than restating the auth setup. Use `send_anon` to assert a
/// route rejects an unauthenticated caller, and `send_authed` to drive a
/// route as a specific role.
pub(crate) async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    send_authed(&app.router, method, uri, body, &manager_token(&app.config)).await
}

/// Sends with no `Authorization` header at all — for asserting that a route
/// is actually gated, and for the one route that is intentionally public.
pub(crate) async fn send_anon(
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
        .expect("the request is well formed");
    execute(router, request).await
}

/// Same as `send_anon`, but with an `Authorization: Bearer <token>` header,
/// so a test can pick the exact role/permission set it wants to exercise.
pub(crate) async fn send_authed(
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
        .expect("the request is well formed");
    execute(router, request).await
}

/// Mints a token carrying a role's *real* default permission set, so the
/// permission gates on the product/stock routes see what they would see
/// after an actual login.
pub(crate) fn token_for_role(config: &Config, role: Role) -> String {
    common::mint_token(config, Some(role), roles::default_permissions(role))
}

/// Manager: holds `inventory:read` + `inventory:write`, but is not Admin.
pub(crate) fn manager_token(config: &Config) -> String {
    token_for_role(config, Role::Manager)
}

/// Inserts a category directly into the test database rather than through
/// the admin API, so tests that don't care about category management
/// itself (e.g. product CRUD) don't need an admin token just to set up
/// fixture data. Uses a random name so parallel tests never collide with
/// each other or with real seeded data. Returns the category's key.
pub(crate) async fn seed_category(db: &mongodb::Database) -> String {
    let name = format!("Test Category {}", Uuid::new_v4());
    let key = generate_id("cat");
    let now = BsonDateTime::now();
    db.collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: key.clone(),
            name,
            icon: "Box".to_string(),
            color: "blue".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .expect("failed to seed category");
    key
}

/// Inserts a subcategory directly into the test database under
/// `category_key`. Returns the subcategory's key.
pub(crate) async fn seed_subcategory(
    db: &mongodb::Database,
    category_key: &str,
    name: &str,
) -> String {
    let key = generate_id("subcat");
    let now = BsonDateTime::now();
    db.collection::<SubcategoryDocument>("subcategories")
        .insert_one(SubcategoryDocument {
            id: None,
            key: key.clone(),
            category_key: category_key.to_string(),
            name: name.to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .expect("failed to seed subcategory");
    key
}

/// Convenience wrapper: seeds a category plus one subcategory under it,
/// returning `(category_key, subcategory_key)` — the shape most product
/// tests need.
pub(crate) async fn seed_category_with_subcategory(
    db: &mongodb::Database,
    subcategory_name: &str,
) -> (String, String) {
    let category_key = seed_category(db).await;
    let subcategory_key = seed_subcategory(db, &category_key, subcategory_name).await;
    (category_key, subcategory_key)
}

/// A unique 10-digit numeric string per call — used for manual-barcode test
/// fixtures so repeated test runs against the persistent test DB (see
/// `common::spawn_app`, which never drops data between runs) never collide
/// with a barcode a previous run already inserted.
pub(crate) fn unique_manual_barcode() -> String {
    let digits: String = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(10)
        .collect();
    format!("{digits:0<10}")
}

// ============================================================================
// Authorization
// ============================================================================
//
// Every route under `/api/inventory` except the module-status stub requires a
// token. These tests exist because the gate is easy to lose: authentication
// is opt-in per handler (a handler that declares no `CurrentUser`/`AdminUser`
// argument is silently public), and these product/stock routes were
// unauthenticated for exactly that reason. Deleting an extractor would still
// compile and still pass every other test in this file, so the assertions
// below are the only thing that would catch it.

/// One representative route per method/permission combination — enough to
/// prove the gate is wired, without restating it for all 19 routes.
pub(crate) const READ_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/inventory/overview"),
    ("GET", "/api/inventory/products"),
    ("GET", "/api/inventory/products/low-stock"),
    ("GET", "/api/inventory/stock-movements"),
    ("GET", "/api/inventory/categories"),
    ("GET", "/api/inventory/categories/valid"),
];
