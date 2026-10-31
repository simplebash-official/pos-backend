use simplebash_pos_backend::core::config::TenantMode;
// `POST /api/internal/provision`: the identity service hands a newly registered
// shop (and, for web sign-ups, its owner's Argon2id hash) to the POS. Runs the real
// router in `TENANT_MODE=multi` against a real MongoDB (MONGODB_URI) in a uniquely
// named throwaway database that is dropped afterwards.

mod common;

use argon2::Argon2;

use argon2::password_hash::{PasswordHasher, SaltString};

use argon2::password_hash::rand_core::OsRng;
use axum::Router;
use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use mongodb::bson::doc;
use serde_json::{Value, json};
use simplebash_pos_backend::clients::indexes::ensure_indexes;
use tower::ServiceExt;

const SECRET: &str = "test-provision-secret-0123456789";
const PASSWORD: &str = "correct-horse-1";
const TENANT_A: &str = "tnt_aaaaaaaaaaaaaaaa";
const TENANT_B: &str = "tnt_bbbbbbbbbbbbbbbb";

fn argon2_hash(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .expect("the password hashes")
        .to_string()
}

async fn post(
    router: &Router,
    uri: &str,
    secret: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(secret) = secret {
        request = request.header("x-provision-secret", secret);
    }
    let response = router
        .clone()
        .oneshot(
            request
                .body(Body::from(body.to_string()))
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get_with_token(router: &Router, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder().method("GET").uri(uri);
    if let Some(t) = token {
        request = request.header("authorization", format!("Bearer {t}"));
    }
    let response = router
        .clone()
        .oneshot(
            request
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn post_with_token(
    router: &Router,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        request = request.header("authorization", format!("Bearer {t}"));
    }
    let response = router
        .clone()
        .oneshot(
            request
                .body(Body::from(body.to_string()))
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn shop(tenant_id: &str, shop_code: &str, owner_hash: Option<&str>) -> Value {
    let mut body = json!({ "tenantId": tenant_id, "shopCode": shop_code, "name": "Ann's Phones" });
    if let Some(hash) = owner_hash {
        body["owner"] = json!({ "username": "Admin", "name": "Ann", "passwordHash": hash });
    }
    body
}

#[tokio::test]
async fn provisioning_creates_the_shop_and_an_owner_who_can_log_in_with_the_same_password() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;

    let (status, body) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some(&argon2_hash(PASSWORD))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["tenantCreated"], true);
    assert_eq!(body["data"]["adminCreated"], true);

    // The owner logs in to the POS with the shop code and the password they chose at sign-up.
    let (status, body) = post(
        &app.router,
        "/api/auth/login",
        None,
        json!({ "username": "admin", "password": PASSWORD, "shopCode": "ann-s-phones" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["user"]["role"], "admin");

    // The Admin belongs to the new tenant, not to the platform scope.
    let stored = app
        .mongo()
        .collection::<mongodb::bson::Document>("users")
        .find_one(doc! { "username": "admin" })
        .await
        .expect("the awaited step succeeds")
        .expect("owner user stored");
    assert_eq!(
        stored
            .get_str("tenant_id")
            .expect("the document has this string field"),
        TENANT_A
    );

    // The new shop starts with the drawer's built-in cash categories, once, even if provisioning is replayed.
    let built_in = simplebash_pos_backend::seeds::cash_categories::SYSTEM_CATEGORIES.len() as u64;
    let categories = app
        .mongo()
        .collection::<mongodb::bson::Document>("cash_categories");
    assert_eq!(
        categories
            .count_documents(doc! { "tenant_id": TENANT_A })
            .await
            .expect("the awaited step succeeds"),
        built_in
    );
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some(&argon2_hash(PASSWORD))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        categories
            .count_documents(doc! { "tenant_id": TENANT_A })
            .await
            .expect("the awaited step succeeds"),
        built_in
    );

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn replaying_a_provisioning_request_changes_nothing() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;
    let hash = argon2_hash(PASSWORD);

    let (first, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some(&hash)),
    )
    .await;
    assert_eq!(first, StatusCode::OK);

    let (status, body) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some(&hash)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["tenantCreated"], false);
    assert_eq!(body["data"]["adminCreated"], false);

    let users = app
        .mongo()
        .collection::<mongodb::bson::Document>("users")
        .count_documents(doc! {})
        .await
        .expect("the awaited step succeeds");
    assert_eq!(users, 1, "a replay must not create a second Admin");

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn a_desktop_sign_up_registers_the_shop_without_an_owner() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;

    let (status, body) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["tenantCreated"], true);
    assert_eq!(body["data"]["adminCreated"], false);

    // The shop code now resolves, but nobody can log in yet: no user was created.
    let (status, _) = post(
        &app.router,
        "/api/auth/login",
        None,
        json!({ "username": "admin", "password": PASSWORD, "shopCode": "ann-s-phones" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn the_shop_code_of_another_shop_is_refused() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;

    let (first, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", None),
    )
    .await;
    assert_eq!(first, StatusCode::OK);

    // Same code, different tenant: must not hijack the first shop's login.
    let (status, body) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(
            TENANT_B,
            "ann-s-phones",
            Some(&argon2_hash("attacker-password-1")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "SHOP_CODE_ALREADY_EXISTS");

    // Same tenant id under a different code is refused too.
    let (status, body) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "another-code", None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "TENANT_ALREADY_EXISTS");

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn bad_input_is_rejected_before_anything_is_written() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;

    // A plaintext password (or any non-Argon2 value) can never be verified, so it is refused.
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some("not-a-hash")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Shop codes follow the POS rules (3-32 chars of a-z, 0-9, dashes).
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ab", None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Tenant ids must be identity's `tnt_...` ids.
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop("not-a-tenant-id", "ann-s-phones", None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let tenants = app
        .mongo()
        .collection::<mongodb::bson::Document>("tenants")
        .count_documents(doc! {})
        .await
        .expect("the awaited step succeeds");
    assert_eq!(tenants, 0);

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn the_shared_secret_is_required_and_the_endpoint_is_off_without_one() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;

    let body = shop(TENANT_A, "ann-s-phones", None);
    let (status, _) = post(&app.router, "/api/internal/provision", None, body.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "missing header");
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some("wrong-secret-0123456789"),
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "wrong secret");
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(&SECRET[..SECRET.len() - 1]),
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a prefix of the secret");

    let tenants = app
        .mongo()
        .collection::<mongodb::bson::Document>("tenants")
        .count_documents(doc! {})
        .await
        .expect("the awaited step succeeds");
    assert_eq!(
        tenants, 0,
        "nothing is written for an unauthenticated caller"
    );
    let _ = app.mongo().drop().await;

    // A deployment with no PROVISION_SECRET behaves as if the route did not exist.
    let off = common::spawn_app_multi_tenant_with_secret(None).await;
    let (status, _) = post(&off.router, "/api/internal/provision", Some(SECRET), body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = off.mongo().drop().await;
}

#[tokio::test]
async fn multi_tenant_onboarding_and_sample_data_seeding() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;

    // 1. Provision shop with owner
    let (status, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some(&argon2_hash(PASSWORD))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 2. Query setup-status by shop code before login
    let (status, body) = get_with_token(
        &app.router,
        "/api/system/setup-status?shop=ann-s-phones",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["setupCompleted"], false);
    assert_eq!(body["data"]["isFirstRun"], true);

    // 3. Log in with owner credentials
    let (status, body) = post(
        &app.router,
        "/api/auth/login",
        None,
        json!({ "username": "admin", "password": PASSWORD, "shopCode": "ann-s-phones" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["data"]["token"]
        .as_str()
        .expect("the value is a string");

    // 4. Query setup-status with bearer token
    let (status, body) = get_with_token(&app.router, "/api/system/setup-status", Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["setupCompleted"], false);

    // 5. Complete setup with sample data
    let (status, body) = post_with_token(
        &app.router,
        "/api/system/setup",
        Some(token),
        json!({ "loadSampleData": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["setupCompleted"], true);
    assert_eq!(body["data"]["sampleDataLoaded"], true);

    // 6. Query setup-status again: should now be completed
    let (status, body) = get_with_token(&app.router, "/api/system/setup-status", Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["setupCompleted"], true);
    assert_eq!(body["data"]["sampleDataLoaded"], true);

    // 7. Verify products exist inside tenant
    let (status, body) = get_with_token(&app.router, "/api/inventory/products", Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    let count = body["data"]["items"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(
        count > 0,
        "sample products should be loaded in tenant scope: {body}"
    );

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn replaying_with_a_new_name_refreshes_names_but_keeps_the_password() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(app.mongo(), TenantMode::Multi).await;
    let hash = argon2_hash(PASSWORD);

    let (first, _) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        shop(TENANT_A, "ann-s-phones", Some(&hash)),
    )
    .await;
    assert_eq!(first, StatusCode::OK);

    // Identity renamed the owner and the shop, and (say) rotated its own hash.
    let renamed = json!({
        "tenantId": TENANT_A,
        "shopCode": "ann-s-phones",
        "name": "Ann Mobile",
        "owner": { "username": "admin", "name": "Ann Perera", "passwordHash": argon2_hash("another-pass-9") },
    });
    let (status, body) = post(
        &app.router,
        "/api/internal/provision",
        Some(SECRET),
        renamed,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["adminCreated"], false);

    let user = app
        .mongo()
        .collection::<mongodb::bson::Document>("users")
        .find_one(doc! { "username": "admin" })
        .await
        .expect("the awaited step succeeds")
        .expect("owner user stored");
    assert_eq!(
        user.get_str("name")
            .expect("the document has this string field"),
        "Ann Perera"
    );

    let tenant = app
        .mongo()
        .collection::<mongodb::bson::Document>("tenants")
        .find_one(doc! { "key": TENANT_A })
        .await
        .expect("the awaited step succeeds")
        .expect("tenant stored");
    assert_eq!(
        tenant
            .get_str("name")
            .expect("the document has this string field"),
        "Ann Mobile"
    );

    // The local password is untouched by a replay.
    let (status, body) = post(
        &app.router,
        "/api/auth/login",
        None,
        json!({ "username": "admin", "password": PASSWORD, "shopCode": "ann-s-phones" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let _ = app.mongo().drop().await;
}
