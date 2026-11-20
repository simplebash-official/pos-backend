// The notification engine end to end: raising from the app (dedupe, audience rules, validation),
// per-user visibility and read state, the history filters and paging, the bell summary, the
// server-side producers (low stock, repair ready), retention, the live stream and the internal
// route trusted internal services use. On SQLite and on a throwaway Atlas DB.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use mongodb::bson::{DateTime as BsonDateTime, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::notifications::service;
use tower::ServiceExt;

fn token(app: &common::TestApp, user: &str, role: Role) -> String {
    let claims = Claims {
        sub: user.to_string(),
        exp: 9_999_999_999,
        role: Some(role),
        permissions: roles::default_permissions(role)
            .iter()
            .map(|p| p.to_string())
            .collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some(user.to_string()),
        pwv: None,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(value) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            builder
                .body(Body::from(value.to_string()))
                .expect("the request is well formed")
        }
        None => builder
            .body(Body::empty())
            .expect("the request is well formed"),
    };
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the body is JSON")
    };
    (status, json)
}

fn text(value: &Value) -> String {
    value.as_str().expect("the value is text").to_string()
}

async fn raise(app: &common::TestApp, token: &str, body: Value) -> (StatusCode, Value) {
    send(app, "POST", "/api/notifications", Some(body), token).await
}

fn alert(title: &str, audience: Value, dedupe: Option<&str>) -> Value {
    json!({
        "category": "expenses",
        "severity": "warning",
        "title": title,
        "body": format!("{title}: please check"),
        "link": { "path": "/expenses", "label": "Open" },
        "audience": audience,
        "dedupeKey": dedupe,
    })
}

async fn summary(app: &common::TestApp, token: &str) -> Value {
    let (status, body) = send(app, "GET", "/api/notifications/summary", None, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

fn titles(items: &Value) -> Vec<String> {
    items
        .as_array()
        .expect("the items are a list")
        .iter()
        .map(|n| text(&n["title"]))
        .collect()
}

async fn scenario(app: &common::TestApp) {
    let staff = token(app, "usr_staff", Role::Staff);
    let other_staff = token(app, "usr_staff2", Role::Staff);
    let manager = token(app, "usr_manager", Role::Manager);

    // ---- Raising: dedupe, audience rules, validation ----
    let (status, first) = raise(
        app,
        &staff,
        alert(
            "Rent due",
            json!({ "type": "everyone" }),
            Some("bill:rent:2026-11"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = raise(
        app,
        &other_staff,
        alert(
            "Rent due",
            json!({ "type": "everyone" }),
            Some("bill:rent:2026-11"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{again}");
    assert_eq!(
        first["data"]["key"], again["data"]["key"],
        "one per dedupe key"
    );

    let (status, body) = raise(
        app,
        &staff,
        alert(
            "Not mine",
            json!({ "type": "permission", "permission": "expenses:view" }),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "staff lacks expenses:view: {body}"
    );
    let (status, body) = raise(
        app,
        &manager,
        alert(
            "Managers only",
            json!({ "type": "permission", "permission": "expenses:view" }),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = raise(app, &staff, alert("For me", json!({ "type": "me" }), None)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body["data"]["audience"],
        json!({ "type": "user", "userKey": "usr_staff" }),
        "the server names the caller"
    );
    // The app has no way to address one other user: `user` is not an app audience.
    let request = Request::builder()
        .method("POST")
        .uri("/api/notifications")
        .header(AUTHORIZATION, format!("Bearer {staff}"))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            alert(
                "For a colleague",
                json!({ "type": "user", "userKey": "usr_staff2" }),
                None,
            )
            .to_string(),
        ))
        .expect("the request is well formed");
    let response = app.router.clone().oneshot(request).await.expect("answers");
    assert!(response.status().is_client_error(), "{}", response.status());

    let mut outside = alert("Phishing", json!({ "type": "everyone" }), None);
    outside["link"] = json!({ "path": "https://evil.example", "label": "Open" });
    let (status, body) = raise(app, &staff, outside).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = raise(
        app,
        &staff,
        alert("  ", json!({ "type": "everyone" }), None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // ---- Visibility and the bell ----
    let staff_bell = summary(app, &staff).await;
    assert_eq!(titles(&staff_bell["latest"]), ["For me", "Rent due"]);
    assert_eq!(staff_bell["unreadTotal"], 2);
    assert_eq!(
        staff_bell["unreadByCategory"],
        json!([{ "category": "expenses", "unread": 2 }])
    );
    let colleague_bell = summary(app, &other_staff).await;
    assert_eq!(titles(&colleague_bell["latest"]), ["Rent due"]);
    let manager_bell = summary(app, &manager).await;
    assert_eq!(
        titles(&manager_bell["latest"]),
        ["Managers only", "Rent due"]
    );

    // ---- Read state is per user ----
    let rent = text(&first["data"]["key"]);
    let manager_only = text(&manager_bell["latest"][0]["key"]);
    let (status, body) = send(
        app,
        "POST",
        "/api/notifications/read",
        Some(json!({ "keys": [rent, manager_only] })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["marked"], 1, "a hidden key is ignored: {body}");
    let (_, body) = send(
        app,
        "POST",
        "/api/notifications/read",
        Some(json!({ "keys": [rent] })),
        &staff,
    )
    .await;
    assert_eq!(body["data"]["marked"], 0, "marking twice changes nothing");
    assert_eq!(summary(app, &staff).await["unreadTotal"], 1);
    assert_eq!(
        summary(app, &manager).await["unreadTotal"],
        2,
        "others keep theirs"
    );
    let (status, _) = send(
        app,
        "POST",
        "/api/notifications/read",
        Some(json!({ "keys": [] })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // ---- History: state, filters, search, paging ----
    let list = |query: &str| format!("/api/notifications?{query}");
    let (status, body) = send(app, "GET", &list("state=unread&limit=10"), None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(titles(&body["data"]["items"]), ["For me"]);
    let (_, body) = send(app, "GET", &list("state=read&limit=10"), None, &staff).await;
    assert_eq!(titles(&body["data"]["items"]), ["Rent due"]);
    assert_eq!(body["data"]["items"][0]["read"], true);
    let (_, body) = send(app, "GET", &list("state=all&limit=10&q=RENT"), None, &staff).await;
    assert_eq!(
        titles(&body["data"]["items"]),
        ["Rent due"],
        "search ignores case"
    );
    let (_, body) = send(
        app,
        "GET",
        &list("state=all&limit=10&categories=repairs"),
        None,
        &staff,
    )
    .await;
    assert!(titles(&body["data"]["items"]).is_empty());
    for bad in [
        "state=all&limit=0",
        "state=all&limit=51",
        "state=all&limit=5&categories=billing",
        "state=all&limit=5&cursor=garbage",
    ] {
        let (status, body) = send(app, "GET", &list(bad), None, &staff).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {body}");
    }

    let (_, page1) = send(app, "GET", &list("state=all&limit=1"), None, &manager).await;
    assert_eq!(titles(&page1["data"]["items"]), ["Managers only"]);
    let cursor = text(&page1["data"]["nextCursor"]);
    let (_, page2) = send(
        app,
        "GET",
        &list(&format!("state=all&limit=1&cursor={cursor}")),
        None,
        &manager,
    )
    .await;
    assert_eq!(titles(&page2["data"]["items"]), ["Rent due"]);
    assert!(
        page2["data"].get("nextCursor").is_none(),
        "the last page has no next: {page2}"
    );

    // ---- Mark all read honours the filter ----
    let (status, body) = send(
        app,
        "POST",
        "/api/notifications/read-all",
        Some(json!({ "q": "managers" })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["marked"], 1);
    assert_eq!(summary(app, &manager).await["unreadTotal"], 1);

    // ---- Producer: a repair that becomes ready tells everyone on repairs ----
    let nimal = common::register_customer(app, "Nimal Perera", "0771234567").await;
    let (status, created) = send(
        app,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": { "customerKey": nimal },
            "deviceModel": "Galaxy A55",
            "issueDescription": "Cracked screen",
            "items": [{ "description": "Clean ports", "laborCents": 500,
                        "warranty": { "coverage": { "kind": "none" } } }],
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let repair = created["data"].clone();
    for _ in 0..2 {
        let (status, body) = send(
            app,
            "PATCH",
            &format!("/api/repairs/{}", text(&repair["id"])),
            Some(json!({ "status": "ready" })),
            &manager,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (_, body) = send(
        app,
        "GET",
        &list("state=unread&limit=10&categories=repairs"),
        None,
        &other_staff,
    )
    .await;
    let items = body["data"]["items"].as_array().expect("a list").clone();
    assert_eq!(
        items.len(),
        1,
        "one notice however often it is saved: {body}"
    );
    assert_eq!(items[0]["title"], "Repair ready for pickup");
    assert_eq!(items[0]["severity"], "success");
    assert_eq!(
        items[0]["link"]["path"],
        format!("/repairs?jobKey={}", text(&repair["key"]))
    );

    // ---- Producer: stock falling to its alert level ----
    let admin = token(app, "usr_admin", Role::Admin);
    let (status, category) = send(
        app,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Parts {}", uuid::Uuid::new_v4()), "icon": "Box", "color": "blue",
            "subcategories": ["Spares"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let (status, product) = send(
        app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Charger",
            "categoryKey": text(&category["data"]["key"]),
            "subcategoryKey": text(&category["data"]["subcategories"][0]["key"]),
            "minStockThreshold": 2,
            "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 4, "unitCostCents": 500, "sellingPriceCents": 900 },
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let product_key = text(&product["data"]["key"]);
    let product_id = text(&product["data"]["id"]);
    let (_, batches) = send(
        app,
        "GET",
        &format!("/api/inventory/products/{product_key}/batches"),
        None,
        &admin,
    )
    .await;
    let batch = text(&batches["data"]["items"][0]["key"]);
    let take = |n: i64| json!({ "delta": -n, "reason": "Counted", "batchKey": batch });
    for (n, expected) in [(1, 0), (1, 1), (1, 1), (1, 2)] {
        let (status, body) = send(
            app,
            "PATCH",
            &format!("/api/inventory/products/{product_id}/stock"),
            Some(take(n)),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, body) = send(
            app,
            "GET",
            &list("state=all&limit=10&categories=inventory"),
            None,
            &staff,
        )
        .await;
        assert_eq!(
            body["data"]["items"].as_array().expect("a list").len(),
            expected,
            "4 -> 3 -> 2 (low) -> 1 (still low) -> 0 (out): {body}"
        );
    }
    let (_, body) = send(
        app,
        "GET",
        &list("state=all&limit=10&categories=inventory&severities=critical"),
        None,
        &staff,
    )
    .await;
    assert_eq!(titles(&body["data"]["items"]), ["Out of stock: Charger"]);

    // ---- Retention: anything older than the window is removed with its read marks ----
    let old = BsonDateTime::from_millis(
        BsonDateTime::now().timestamp_millis()
            - (service::NOTIFICATION_RETENTION_DAYS + 1) * 24 * 3600 * 1000,
    );
    match app.db_handle.as_sqlite() {
        Some(pool) => {
            sqlx::query("UPDATE notifications SET created_at = ? WHERE key = ?")
                .bind(old.to_chrono().to_rfc3339())
                .bind(&rent)
                .execute(pool)
                .await
                .expect("the row ages");
        }
        None => {
            app.mongo()
                .collection::<mongodb::bson::Document>("notifications")
                .update_one(
                    doc! { "key": &rent },
                    doc! { "$set": { "created_at": old } },
                )
                .await
                .expect("the row ages");
        }
    }
    let removed = service::purge_expired(&app.db_handle)
        .await
        .expect("the purge runs");
    assert_eq!(removed, 1);
    let (_, body) = send(app, "GET", &list("state=read&limit=10"), None, &staff).await;
    assert!(
        titles(&body["data"]["items"]).is_empty(),
        "the read mark went with it: {body}"
    );
}

#[tokio::test]
async fn notifications_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn notifications_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn the_live_stream_says_hello_and_needs_a_token() {
    use futures_util::StreamExt;

    let app = common::spawn_app_sqlite().await;
    let staff = token(&app, "usr_staff", Role::Staff);
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications/events")
                .header(AUTHORIZATION, format!("Bearer {staff}"))
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/event-stream",
        "an SSE stream"
    );
    let mut body = response.into_body().into_data_stream();
    let first = body
        .next()
        .await
        .expect("a first event")
        .expect("the stream reads");
    assert!(
        String::from_utf8_lossy(&first).contains("event: hello"),
        "{first:?}"
    );

    let (status, _) = send(&app, "GET", "/api/notifications/events", None, "nope").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

const SECRET: &str = "test-provision-secret-0123456789";

async fn internal(
    app: &common::TestApp,
    tenant: &str,
    secret: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/internal/tenants/{tenant}/notifications"))
        .header(CONTENT_TYPE, "application/json");
    if let Some(secret) = secret {
        builder = builder.header("x-provision-secret", secret);
    }
    let response = app
        .router
        .clone()
        .oneshot(
            builder
                .body(Body::from(body.to_string()))
                .expect("the request is well formed"),
        )
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn notice(dedupe: &str, audience: Value) -> Value {
    json!({
        "category": "announcements",
        "severity": "info",
        "title": "New feature",
        "body": "You can now print warranty cards.",
        "audience": audience,
        "dedupeKey": dedupe,
    })
}

#[tokio::test]
async fn a_trusted_service_notifies_one_shop_only() {
    let app = common::spawn_app_multi_tenant_with_secret(Some(SECRET)).await;
    let shop = format!("tnt_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]);
    let other = format!("tnt_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]);
    for (tenant, code) in [(&shop, "ann-notice"), (&other, "bob-notice")] {
        let response = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/internal/provision")
                    .header(CONTENT_TYPE, "application/json")
                    .header("x-provision-secret", SECRET)
                    .body(Body::from(
                        json!({ "tenantId": tenant, "shopCode": code, "name": code }).to_string(),
                    ))
                    .expect("the request is well formed"),
            )
            .await
            .expect("the router answers");
        assert_eq!(response.status(), StatusCode::OK);
    }
    let everyone = json!({ "type": "everyone" });

    let (status, _) = internal(&app, &shop, None, notice("n1", everyone.clone())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = internal(
        &app,
        &shop,
        Some("wrong-secret-0123456789"),
        notice("n1", everyone.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = internal(
        &app,
        "tnt_missing",
        Some(SECRET),
        notice("n1", everyone.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "TENANT_NOT_FOUND");
    let (status, body) = internal(
        &app,
        &shop,
        Some(SECRET),
        notice("n1", json!({ "type": "user", "userKey": "usr_1" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = internal(
        &app,
        &shop,
        Some(SECRET),
        notice(
            "n1",
            json!({ "type": "permission", "permission": "nope:view" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    for _ in 0..2 {
        let (status, body) =
            internal(&app, &shop, Some(SECRET), notice("n1", everyone.clone())).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["data"]["source"], "service");
    }

    let reader = |tenant: &str| {
        common::mint_token_for_tenant(
            &app.config,
            Some(Role::Staff),
            roles::default_permissions(Role::Staff),
            Some(tenant),
        )
    };
    let (status, body) = send(
        &app,
        "GET",
        "/api/notifications/summary",
        None,
        &reader(&shop),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        titles(&body["data"]["latest"]),
        ["New feature"],
        "sent once"
    );
    let (_, body) = send(
        &app,
        "GET",
        "/api/notifications/summary",
        None,
        &reader(&other),
    )
    .await;
    assert!(
        titles(&body["data"]["latest"]).is_empty(),
        "other shops see nothing"
    );
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn the_internal_route_is_absent_on_a_single_shop_install() {
    let app = common::spawn_app_sqlite().await;
    let (status, _) = internal(
        &app,
        "tnt_any",
        Some(SECRET),
        notice("n1", json!({ "type": "everyone" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
