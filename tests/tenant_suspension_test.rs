// Suspending a shop (`PUT /api/internal/tenants/{id}/status`) and the usage figures
// (`GET /api/internal/tenants/{id}/usage`, `POST /api/internal/tenants/usage`). Real router in
// `TENANT_MODE=multi` against a real MongoDB (MONGODB_URI) in a throwaway database.

mod common;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header::CONTENT_TYPE},
};
use mongodb::bson::{DateTime, Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::indexes::ensure_indexes;
use tower::ServiceExt;

const SECRET: &str = "test-provision-secret-0123456789";
const PASSWORD: &str = "correct-horse-1";
const DAY_MS: i64 = 24 * 3600 * 1000;

fn has_mongo() -> bool {
    dotenvy::dotenv().ok();
    if std::env::var("MONGODB_URI").is_err() {
        eprintln!("MONGODB_URI not set - skipping");
        return false;
    }
    true
}

/// A tenant id no other test (running in the same process, sharing the suspension cache) uses.
fn tenant_id() -> String {
    format!("tnt_{}", &uuid::Uuid::new_v4().simple().to_string()[..16])
}

fn argon2_hash(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string()
}

async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    secret: Option<&str>,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if body.is_some() {
        request = request.header(CONTENT_TYPE, "application/json");
    }
    if let Some(secret) = secret {
        request = request.header("x-provision-secret", secret);
    }
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(
            request
                .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn provision(router: &Router, tenant: &str, code: &str) {
    let (status, body) = send(
        router,
        "POST",
        "/api/internal/provision",
        Some(SECRET),
        None,
        Some(json!({
            "tenantId": tenant, "shopCode": code, "name": "Ann's Phones",
            "owner": { "username": "admin", "name": "Ann", "passwordHash": argon2_hash(PASSWORD) },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn login(router: &Router, code: &str, password: &str) -> (StatusCode, Value) {
    send(
        router,
        "POST",
        "/api/auth/login",
        None,
        None,
        Some(json!({ "username": "admin", "password": password, "shopCode": code })),
    )
    .await
}

const SINCE: &str = "2026-10-01T08:30:00Z";
const REASON: &str = "unpaid invoice";

async fn put_status(router: &Router, tenant: &str, body: Value) -> (StatusCode, Value) {
    send(
        router,
        "PUT",
        &format!("/api/internal/tenants/{tenant}/status"),
        Some(SECRET),
        None,
        Some(body),
    )
    .await
}

async fn set_status(router: &Router, tenant: &str, suspended: bool) -> (StatusCode, Value) {
    let body = if suspended {
        json!({ "suspended": true, "suspendedAt": SINCE, "reason": REASON })
    } else {
        json!({ "suspended": false })
    };
    put_status(router, tenant, body).await
}

#[tokio::test]
async fn a_suspended_shop_is_refused_everywhere_and_can_be_reinstated() {
    if !has_mongo() {
        return;
    }
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(&app.db, true).await;
    let (tenant, other) = (tenant_id(), tenant_id());
    provision(&app.router, &tenant, "ann-s-phones").await;
    provision(&app.router, &other, "bob-repairs").await;

    // Signed in and working before the suspension.
    let (status, body) = login(&app.router, "ann-s-phones", PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token = body["data"]["token"].as_str().unwrap().to_string();
    let device = common::mint_device_token(&app.config, &tenant, "dev-1");
    let me = |t: String| {
        let router = app.router.clone();
        async move { send(&router, "GET", "/api/auth/me", None, Some(&t), None).await }
    };
    assert_eq!(me(token.clone()).await.0, StatusCode::OK);

    let (status, body) = set_status(&app.router, &tenant, true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["suspended"], true);

    // An already-issued staff token, a device token and the shop-code login are all refused.
    let (status, body) = me(token.clone()).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("ACCOUNT_SUSPENDED")),
        "{body}"
    );
    // The refusal carries what the suspended page shows: identity's time, the reason, and proof
    // the shop's data is still stored (the shop has its Admin, so at least one staff account).
    let details = &body["details"];
    assert_eq!(details["suspendedAt"], SINCE, "{body}");
    assert_eq!(details["reason"], REASON, "{body}");
    assert!(
        details["shopName"].as_str().is_some_and(|n| !n.is_empty()),
        "{body}"
    );
    assert!(
        details["retained"]["staff"].as_u64().unwrap() >= 1,
        "{body}"
    );
    assert_eq!(details["retained"]["bills"], 0, "{body}");
    let (status, body) = send(
        &app.router,
        "GET",
        "/api/sync/pull?since=0",
        None,
        Some(&device),
        None,
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("ACCOUNT_SUSPENDED")),
        "{body}"
    );
    let (status, body) = login(&app.router, "ann-s-phones", PASSWORD).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("ACCOUNT_SUSPENDED")),
        "{body}"
    );
    assert_eq!(body["details"]["reason"], REASON, "{body}");
    // ...but a stranger guessing the password learns nothing about the suspension.
    let (status, body) = login(&app.router, "ann-s-phones", "wrong-password-1").await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("INVALID_CREDENTIALS")),
        "{body}"
    );

    // Re-sending the suspension keeps identity's time (the POS never re-stamps it) and takes the
    // new reason at once on this instance.
    let (status, body) = put_status(
        &app.router,
        &tenant,
        json!({ "suspended": true, "suspendedAt": SINCE, "reason": "second notice" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = me(token.clone()).await;
    assert_eq!(body["details"]["suspendedAt"], SINCE, "{body}");
    assert_eq!(body["details"]["reason"], "second notice", "{body}");

    // Another shop is untouched.
    let (status, body) = login(&app.router, "bob-repairs", PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Public routes still answer.
    let (status, _) = send(&app.router, "GET", "/api/health", None, None, None).await;
    assert_eq!(status, StatusCode::OK);

    // Reinstated: the very same token works again; setting it twice is harmless.
    assert_eq!(
        set_status(&app.router, &tenant, false).await.0,
        StatusCode::OK
    );
    assert_eq!(
        set_status(&app.router, &tenant, false).await.0,
        StatusCode::OK
    );
    assert_eq!(me(token).await.0, StatusCode::OK);
    assert_eq!(
        login(&app.router, "ann-s-phones", PASSWORD).await.0,
        StatusCode::OK
    );

    let _ = app.db.drop().await;
}

#[tokio::test]
async fn the_status_route_needs_the_secret_and_a_known_shop() {
    if !has_mongo() {
        return;
    }
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(&app.db, true).await;
    let tenant = tenant_id();
    provision(&app.router, &tenant, "ann-s-phones").await;

    for (method, uri, body) in [
        (
            "PUT",
            format!("/api/internal/tenants/{tenant}/status"),
            Some(json!({ "suspended": true })),
        ),
        ("GET", format!("/api/internal/tenants/{tenant}/usage"), None),
        (
            "POST",
            "/api/internal/tenants/usage".to_string(),
            Some(json!({ "tenantIds": [tenant] })),
        ),
    ] {
        let (status, _) = send(&app.router, method, &uri, None, None, body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: no secret"
        );
        let (status, _) = send(
            &app.router,
            method,
            &uri,
            Some("wrong-secret-0123456789"),
            None,
            body,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: wrong secret"
        );
    }
    let stored = app
        .db
        .collection::<Document>("tenants")
        .find_one(doc! { "key": &tenant })
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored.get("suspended_at").is_none(),
        "an unauthorised call changed nothing"
    );

    let (status, body) = set_status(&app.router, "tnt_doesnotexist0000", true).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("TENANT_NOT_FOUND")),
        "{body}"
    );
    let (status, _) = set_status(&app.router, "not-a-tenant", true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The body must match `suspended`: a suspension needs its time and reason, a reinstatement
    // carries neither.
    for body in [
        json!({ "suspended": true }),
        json!({ "suspended": true, "suspendedAt": SINCE }),
        json!({ "suspended": true, "reason": REASON }),
        json!({ "suspended": true, "suspendedAt": SINCE, "reason": "   " }),
        json!({ "suspended": true, "suspendedAt": SINCE, "reason": "x".repeat(501) }),
        json!({ "suspended": true, "suspendedAt": "yesterday", "reason": REASON }),
        json!({ "suspended": false, "reason": REASON }),
        json!({ "suspended": false, "suspendedAt": SINCE }),
    ] {
        let (status, resp) = put_status(&app.router, &tenant, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {resp}");
    }
    let still_active = app
        .db
        .collection::<Document>("tenants")
        .find_one(doc! { "key": &tenant })
        .await
        .unwrap()
        .unwrap();
    assert!(
        still_active.get("suspended_at").is_none()
            && still_active.get("suspended_reason").is_none(),
        "a rejected request changed nothing"
    );

    // Without a configured secret the routes do not exist.
    let off = common::spawn_app_multi_tenant_with_secret(None).await;
    let (status, _) = send(
        &off.router,
        "PUT",
        &format!("/api/internal/tenants/{tenant}/status"),
        Some(SECRET),
        None,
        Some(json!({ "suspended": true })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let _ = app.db.drop().await;
    let _ = off.db.drop().await;
}

#[tokio::test]
async fn usage_counts_this_shop_only_and_leaves_out_voided_and_deleted_rows() {
    if !has_mongo() {
        return;
    }
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(&app.db, true).await;
    let (tenant, other) = (tenant_id(), tenant_id());
    provision(&app.router, &tenant, "ann-s-phones").await;
    provision(&app.router, &other, "bob-repairs").await;

    let now = DateTime::now().timestamp_millis();
    let at = |days: i64| DateTime::from_millis(now - days * DAY_MS);
    let insert = |name: &'static str, docs: Vec<Document>| {
        let db = app.db.clone();
        async move {
            db.collection::<Document>(name)
                .insert_many(docs)
                .await
                .unwrap()
        }
    };
    let invoice = |t: &str, status: &str, cents: i64, when: DateTime| {
        doc! { "tenant_id": t, "status": status, "total_cents": cents, "created_at": when }
    };
    insert(
        "invoices",
        vec![
            invoice(&tenant, "paid", 10_000, at(0)),
            invoice(&tenant, "paid", 5_000, at(3)),
            invoice(&tenant, "pending", 2_500, at(20)),
            invoice(&tenant, "paid", 40_000, at(60)),
            invoice(&tenant, "voided", 99_999, at(1)),
            invoice(&other, "paid", 777_000, at(0)),
        ],
    )
    .await;
    insert(
        "products",
        vec![
            doc! { "tenant_id": &tenant, "name": "a" },
            doc! { "tenant_id": &tenant, "name": "b", "deleted_at": null },
            doc! { "tenant_id": &tenant, "name": "gone", "deleted_at": at(1) },
            doc! { "tenant_id": &other, "name": "c" },
        ],
    )
    .await;
    insert(
        "customers",
        vec![
            doc! { "tenant_id": &tenant, "name": "x" },
            doc! { "tenant_id": &other, "name": "y" },
        ],
    )
    .await;
    insert(
        "sync_device_state",
        vec![
            doc! { "tenant_id": &tenant, "device_id": "d1", "revoked": false, "last_seen": at(2) },
            doc! { "tenant_id": &tenant, "device_id": "d2", "revoked": false, "last_seen": at(0) },
            doc! { "tenant_id": &tenant, "device_id": "d3", "revoked": true, "last_seen": at(0) },
            doc! { "tenant_id": &other, "device_id": "d9", "revoked": false, "last_seen": at(0) },
        ],
    )
    .await;

    let (status, body) = send(
        &app.router,
        "GET",
        &format!("/api/internal/tenants/{tenant}/usage"),
        Some(SECRET),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let u = &body["data"];
    assert_eq!(u["tenantId"], tenant.as_str());
    assert_eq!(u["shopCode"], "ann-s-phones");
    assert_eq!(u["suspended"], false);
    assert_eq!(
        (u["billsTotal"].clone(), u["salesTotalCents"].clone()),
        (json!(4), json!(57_500)),
        "voided and other shops excluded: {u}"
    );
    assert_eq!(
        (u["bills30d"].clone(), u["sales30dCents"].clone()),
        (json!(3), json!(17_500)),
        "{u}"
    );
    assert_eq!(u["bills7d"], 2);
    assert_eq!(
        u["products"], 2,
        "the soft-deleted product is not counted: {u}"
    );
    assert_eq!(u["customers"], 1);
    assert_eq!(u["users"], 1, "the provisioned admin: {u}");
    assert_eq!(u["devices"], 2, "revoked devices are not counted: {u}");
    assert!(
        u["lastSyncAt"].is_string() && u["lastBillAt"].is_string(),
        "{u}"
    );
    let days = u["billsByDay"].as_array().unwrap();
    assert_eq!(days.len(), 3, "{u}");
    assert_eq!(
        days.iter()
            .map(|d| d["bills"].as_i64().unwrap())
            .sum::<i64>(),
        3
    );
    assert!(
        days.windows(2)
            .all(|w| w[0]["date"].as_str() < w[1]["date"].as_str()),
        "oldest first: {u}"
    );

    // The batch form returns the same figures and skips unknown ids.
    let (status, body) = send(
        &app.router,
        "POST",
        "/api/internal/tenants/usage",
        Some(SECRET),
        None,
        Some(json!({ "tenantIds": [tenant, other, "tnt_doesnotexist0000"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["billsTotal"], 4);
    assert_eq!(items[1]["billsTotal"], 1);
    assert_eq!(items[1]["salesTotalCents"], 777_000);

    // Suspension shows in the figures.
    set_status(&app.router, &tenant, true).await;
    let (_, body) = send(
        &app.router,
        "GET",
        &format!("/api/internal/tenants/{tenant}/usage"),
        Some(SECRET),
        None,
        None,
    )
    .await;
    assert_eq!(body["data"]["suspended"], true);

    let too_many: Vec<String> = (0..101).map(|i| format!("tnt_{i:016}")).collect();
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/internal/tenants/usage",
        Some(SECRET),
        None,
        Some(json!({ "tenantIds": too_many })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = send(
        &app.router,
        "GET",
        "/api/internal/tenants/tnt_doesnotexist0000/usage",
        Some(SECRET),
        None,
        None,
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("TENANT_NOT_FOUND"))
    );

    let _ = app.db.drop().await;
}

#[tokio::test]
async fn a_suspended_shop_reports_what_it_still_holds_and_reinstating_changes_none_of_it() {
    if !has_mongo() {
        return;
    }
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    ensure_indexes(&app.db, true).await;
    let (tenant, other) = (tenant_id(), tenant_id());
    provision(&app.router, &tenant, "ann-s-phones").await;
    provision(&app.router, &other, "bob-repairs").await;

    let now = DateTime::now();
    let insert = |name: &'static str, docs: Vec<Document>| {
        let db = app.db.clone();
        async move {
            db.collection::<Document>(name)
                .insert_many(docs)
                .await
                .unwrap()
        }
    };
    insert(
        "invoices",
        vec![
            doc! { "tenant_id": &tenant, "status": "paid", "total_cents": 1_000, "created_at": now },
            doc! { "tenant_id": &tenant, "status": "paid", "total_cents": 2_000, "created_at": now },
            doc! { "tenant_id": &tenant, "status": "voided", "total_cents": 9_000, "created_at": now },
            doc! { "tenant_id": &other, "status": "paid", "total_cents": 5_000, "created_at": now },
        ],
    )
    .await;
    insert(
        "products",
        vec![
            doc! { "tenant_id": &tenant, "name": "a" },
            doc! { "tenant_id": &tenant, "name": "b" },
            doc! { "tenant_id": &tenant, "name": "gone", "deleted_at": now },
            doc! { "tenant_id": &other, "name": "c" },
        ],
    )
    .await;
    insert(
        "customers",
        vec![
            doc! { "tenant_id": &tenant, "name": "x" },
            doc! { "tenant_id": &other, "name": "y" },
        ],
    )
    .await;

    let (_, login_body) = login(&app.router, "ann-s-phones", PASSWORD).await;
    let token = login_body["data"]["token"].as_str().unwrap().to_string();

    assert_eq!(
        set_status(&app.router, &tenant, true).await.0,
        StatusCode::OK
    );
    let (status, body) = send(&app.router, "GET", "/api/auth/me", None, Some(&token), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let retained = &body["details"]["retained"];
    assert_eq!(
        retained["bills"], 2,
        "voided and other shops' bills are not counted: {body}"
    );
    assert_eq!(retained["products"], 2, "{body}");
    assert_eq!(retained["customers"], 1, "{body}");
    assert!(retained["lastBillAt"].is_string(), "{body}");

    // Reinstating touches nothing: the same rows are there and the same token works.
    assert_eq!(
        set_status(&app.router, &tenant, false).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&app.router, "GET", "/api/auth/me", None, Some(&token), None)
            .await
            .0,
        StatusCode::OK
    );
    let kept = |name: &'static str| {
        let db = app.db.clone();
        let tenant = tenant.clone();
        async move {
            db.collection::<Document>(name)
                .count_documents(doc! { "tenant_id": tenant })
                .await
                .unwrap()
        }
    };
    assert_eq!(
        (
            kept("invoices").await,
            kept("products").await,
            kept("customers").await
        ),
        (3, 3, 1)
    );

    let _ = app.db.drop().await;
}
