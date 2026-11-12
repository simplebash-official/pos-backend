// Tenant directory + multi-tenant login, against a real MongoDB (MONGODB_URI)
// in a uniquely named throwaway database that is dropped afterwards. Both
// tenants deliberately use the SAME `admin` username with different passwords, so a
// login that ignored the shop code (or leaked across tenants) would succeed
// where it must not.

mod common;

use std::sync::Arc;

use axum::body::Body;

use axum::http::{Request, StatusCode, header};
use jsonwebtoken::{DecodingKey, Validation, decode};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::app::AppState;
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::clients::document_server::DocumentServerClient;
use simplebash_pos_backend::clients::mongo::connect;
use simplebash_pos_backend::clients::tenant_db::TenantDatabase;
use simplebash_pos_backend::core::config::{SeedAdminConfig, TenantMode};
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::core::tenancy::{DENY_TENANT, Tenant, with_tenant};
use simplebash_pos_backend::domain::users::{CreateUserRequest, Role};
use simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories;
use simplebash_pos_backend::modules::cash_settings::service::ensure_settings;
use simplebash_pos_backend::modules::reports::engine::AnalyticsEngine;
use simplebash_pos_backend::modules::settings::service::ensure_shop_profile;
use simplebash_pos_backend::modules::tenants::service::{
    create_tenant, create_tenant_with_key, ensure_existing_tenants_data, resolve_shop_code,
};
use simplebash_pos_backend::modules::users::service::create_user;
use simplebash_pos_backend::{app, clients, seeds};
use tower::ServiceExt;
use uuid::Uuid;

async fn post_login(router: &axum::Router, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&body).expect("the value serializes to JSON"),
        ))
        .expect("the value serializes to JSON");
    let response = router
        .clone()
        .oneshot(request)
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

#[tokio::test]
async fn tenant_directory_and_multi_tenant_login_roundtrip() {
    dotenvy::dotenv().ok();
    let uri = common::mongo_uri();
    let name = format!("jtdir_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let mut overrides = common::mongo_overrides(&uri, &name).to_vec();
    overrides.push(("TENANT_MODE", "multi"));
    let config = common::test_config(&overrides);
    let raw = connect(&uri, &name).await.expect("connect throwaway db");
    clients::indexes::ensure_indexes(&raw, TenantMode::Multi).await;
    let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

    // --- directory ---------------------------------------------------------
    let a = create_tenant(&db, "  Shop-Alpha ", "Alpha Repairs")
        .await
        .expect("the awaited step succeeds");
    let b = create_tenant(&db, "shop-beta", "Beta Retail")
        .await
        .expect("the awaited step succeeds");
    assert_eq!(
        a.shop_code, "shop-alpha",
        "codes are lowercased and trimmed"
    );
    assert!(a.key.starts_with("tnt_"));
    assert_ne!(a.key, b.key);

    let dup = create_tenant(&db, "SHOP-ALPHA", "Another")
        .await
        .unwrap_err();
    assert!(
        format!("{dup:?}").contains("SHOP_CODE_ALREADY_EXISTS"),
        "{dup:?}"
    );
    assert!(create_tenant(&db, "no", "Too short").await.is_err());

    let resolved = resolve_shop_code(&db, "Shop-Alpha")
        .await
        .expect("the awaited step succeeds");
    assert_eq!(
        resolved,
        Tenant::id(&a.key).expect("the tenant id is valid")
    );
    // A tenant registered under the id the identity service issued keeps that id,
    // so a staff login by shop code reaches the data the devices sync.
    let issued = create_tenant_with_key(
        &db,
        "tnt_issuedbyidentity1".to_string(),
        "test-shop",
        "Test Shop",
    )
    .await
    .expect("the awaited step succeeds");
    assert_eq!(issued.key, "tnt_issuedbyidentity1");
    assert_eq!(
        resolve_shop_code(&db, "test-shop")
            .await
            .expect("the awaited step succeeds"),
        Tenant::id("tnt_issuedbyidentity1").expect("the tenant id is valid")
    );
    for bad in ["issued", "tnt_", "tnt_has space"] {
        assert!(
            create_tenant_with_key(&db, bad.to_string(), "other-shop", "Other")
                .await
                .is_err(),
            "{bad:?} must be rejected as a tenant id"
        );
    }

    let unknown = resolve_shop_code(&db, "nobody-here").await.unwrap_err();
    assert!(
        format!("{unknown:?}").contains("TENANT_NOT_FOUND"),
        "{unknown:?}"
    );

    // --- same `admin` username in both tenants ------------------------------------
    for (info, name, password) in [
        (&a, "Alpha Admin", "alpha-password-1"),
        (&b, "Beta Admin", "beta-password-2"),
    ] {
        with_tenant(
            Tenant::id(&info.key).expect("the tenant id is valid"),
            async {
                create_user(
                    &db,
                    CreateUserRequest {
                        name: name.to_string(),
                        username: "admin".to_string(),
                        password: password.to_string(),
                        role: Role::Admin,
                        employee_key: None,
                    },
                )
                .await
                .expect("each tenant can own the same username")
            },
        )
        .await;
    }

    // --- HTTP login ----------------------------------------------------------
    let config = Arc::new(config);
    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        document_server: Arc::new(DocumentServerClient::new(
            config.document_server_url.clone(),
            config.document_server_api_key.clone(),
        )),
        reports_engine: Arc::new(AnalyticsEngine::new(db.clone())),
    };
    let router = app::build_router(state);

    let claims_of = |token: &str| {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .expect("the step succeeds")
        .claims
    };

    let (status, body) = post_login(
        &router,
        json!({ "username": "admin", "password": "alpha-password-1", "shopCode": "shop-alpha" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token_a = body["data"]["token"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(body["data"]["user"]["name"], "Alpha Admin");
    assert_eq!(claims_of(&token_a).tid.as_deref(), Some(a.key.as_str()));

    let (status, body) = post_login(
        &router,
        json!({ "username": "admin", "password": "beta-password-2", "shopCode": "SHOP-BETA" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["user"]["name"], "Beta Admin");
    assert_eq!(
        claims_of(
            body["data"]["token"]
                .as_str()
                .expect("the value is a string")
        )
        .tid
        .as_deref(),
        Some(b.key.as_str())
    );

    // Alpha's password does not work in Beta's shop, and vice versa.
    let (status, _) = post_login(
        &router,
        json!({ "username": "admin", "password": "alpha-password-1", "shopCode": "shop-beta" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Unknown shop looks exactly like a wrong password; a missing code is a 400.
    let (status, _) = post_login(
        &router,
        json!({ "username": "admin", "password": "alpha-password-1", "shopCode": "ghost-shop" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = post_login(
        &router,
        json!({ "username": "admin", "password": "alpha-password-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The token works for `me` and resolves to the right tenant's account.
    let me = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/auth/me")
                .header(header::AUTHORIZATION, format!("Bearer {token_a}"))
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    assert_eq!(me.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(me.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let me: Value = serde_json::from_slice(&bytes).expect("the text is valid JSON");
    assert_eq!(me["data"]["name"], "Alpha Admin");

    // Public shop lookup endpoint resolves branding name without authentication.
    let lookup_ok = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/auth/shop/shop-alpha")
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    assert_eq!(lookup_ok.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(lookup_ok.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let shop: Value = serde_json::from_slice(&bytes).expect("the text is valid JSON");
    assert_eq!(shop["data"]["shopCode"], "shop-alpha");
    assert_eq!(shop["data"]["name"], "Alpha Repairs");

    let lookup_missing = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/auth/shop/ghost-shop")
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    assert_eq!(lookup_missing.status(), StatusCode::NOT_FOUND);

    raw.drop().await.ok();
}

#[tokio::test]
async fn multi_tenant_cors_only_allows_configured_origins() {
    dotenvy::dotenv().ok();
    let uri = common::mongo_uri();
    let name = format!("jtdir_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let mut overrides = common::mongo_overrides(&uri, &name).to_vec();
    overrides.push(("TENANT_MODE", "multi"));
    overrides.push(("CORS_ALLOWED_ORIGINS", "https://app.example.test"));
    let config = common::test_config(&overrides);
    let raw = connect(&uri, &name).await.expect("connect throwaway db");
    let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));
    let config = Arc::new(config);
    let router = app::build_router(AppState {
        config: config.clone(),
        db: db.clone(),
        document_server: Arc::new(DocumentServerClient::new(
            config.document_server_url.clone(),
            config.document_server_api_key.clone(),
        )),
        reports_engine: Arc::new(AnalyticsEngine::new(db.clone())),
    });

    let allow_origin = |origin: &'static str| {
        let router = router.clone();
        async move {
            let response = router
                .oneshot(
                    Request::builder()
                        .uri("/api/health")
                        .header(header::ORIGIN, origin)
                        .body(Body::empty())
                        .expect("the request is well formed"),
                )
                .await
                .expect("the awaited step succeeds");
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok().map(str::to_string))
        }
    };
    assert_eq!(
        allow_origin("https://app.example.test").await.as_deref(),
        Some("https://app.example.test")
    );
    assert_eq!(allow_origin("https://evil.example.test").await, None);

    raw.drop().await.ok();
}

async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(payload) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&payload).expect("the payload serializes to JSON"),
            ))
            .expect("the request is well formed"),
        None => builder
            .body(Body::empty())
            .expect("the request is well formed"),
    };
    let response = router
        .clone()
        .oneshot(request)
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

#[tokio::test]
async fn deny_scope_refuses_initialization_and_writes_zero_deny_docs() {
    dotenvy::dotenv().ok();
    let uri = common::mongo_uri();
    let name = format!("jtdir_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let raw = connect(&uri, &name).await.expect("connect throwaway db");
    clients::indexes::ensure_indexes(&raw, TenantMode::Multi).await;
    let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

    assert!(
        ensure_shop_profile(&db, Some("Ghost Shop")).await.is_err(),
        "ensure_shop_profile must refuse Tenant::Deny"
    );
    assert!(
        ensure_settings(&db).await.is_err(),
        "ensure_settings must refuse Tenant::Deny"
    );
    assert!(
        ensure_system_categories(&db).await.is_err(),
        "ensure_system_categories must refuse Tenant::Deny"
    );
    assert!(
        seeds::seed_all(
            &db,
            &SeedAdminConfig {
                name: "Admin".to_string(),
                password: "password123".to_string(),
            },
        )
        .await
        .is_err(),
        "seeds::seed_all must refuse Tenant::Deny"
    );

    for collection in [
        "shop_profiles",
        "cash_settings",
        "cash_categories",
        "users",
        "categories",
        "products",
        "customers",
        "suppliers",
        "api_keys",
    ] {
        let deny_count = raw
            .collection::<Document>(collection)
            .count_documents(doc! { "tenant_id": DENY_TENANT })
            .await
            .expect("count deny docs");
        assert_eq!(
            deny_count, 0,
            "collection {collection} must contain zero {DENY_TENANT} rows"
        );
    }

    raw.drop().await.ok();
}

#[tokio::test]
async fn ensure_existing_tenants_data_backfills_missing_profiles_and_unblocks_staff_login() {
    dotenvy::dotenv().ok();
    let uri = common::mongo_uri();
    let name = format!("jtdir_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let mut overrides = common::mongo_overrides(&uri, &name).to_vec();
    overrides.push(("TENANT_MODE", "multi"));
    let config = Arc::new(common::test_config(&overrides));
    let raw = connect(&uri, &name).await.expect("connect throwaway db");
    clients::indexes::ensure_indexes(&raw, TenantMode::Multi).await;
    let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

    // Tenant 1: pre-existing shop missing `shop_profiles`, `cash_settings`, and `cash_categories`,
    // with an existing Staff user.
    let legacy = create_tenant(&db, "shop-legacy", "Legacy Electronics")
        .await
        .expect("create legacy tenant");
    with_tenant(Tenant::id(&legacy.key).expect("valid tenant id"), async {
        create_user(
            &db,
            CreateUserRequest {
                name: "Legacy Cashier".to_string(),
                username: "cashier1".to_string(),
                password: "staff-password-1".to_string(),
                role: Role::Staff,
                employee_key: None,
            },
        )
        .await
        .expect("create staff user")
    })
    .await;

    // Tenant 2: pre-existing shop with customized `shop_profiles` that must remain untouched.
    let custom = create_tenant(&db, "shop-custom", "Custom Directory Name")
        .await
        .expect("create custom tenant");
    with_tenant(Tenant::id(&custom.key).expect("valid tenant id"), async {
        ensure_shop_profile(&db, None).await?;
        ensure_system_categories(&db).await?;
        create_user(
            &db,
            CreateUserRequest {
                name: "Custom Admin".to_string(),
                username: "admin".to_string(),
                password: "custom-password-1".to_string(),
                role: Role::Admin,
                employee_key: None,
            },
        )
        .await
    })
    .await
    .expect("seed custom shop");

    // Tenant 3: pre-existing shop whose `shop_profiles` exists with blank `trading_name`
    // and `allow_shared_user_sessions: true`; backfill populates `trading_name` while keeping mode.
    let blank = create_tenant(&db, "shop-blank", "Blank Profile Name")
        .await
        .expect("create blank tenant");
    with_tenant(Tenant::id(&blank.key).expect("valid tenant id"), async {
        ensure_shop_profile(&db, None).await?;
        ensure_system_categories(&db).await?;
        create_user(
            &db,
            CreateUserRequest {
                name: "Blank Admin".to_string(),
                username: "admin".to_string(),
                password: "blank-password-1".to_string(),
                role: Role::Admin,
                employee_key: None,
            },
        )
        .await
    })
    .await
    .expect("seed blank shop");

    let router = app::build_router(AppState {
        config: config.clone(),
        db: db.clone(),
        document_server: Arc::new(DocumentServerClient::new(
            config.document_server_url.clone(),
            config.document_server_api_key.clone(),
        )),
        reports_engine: Arc::new(AnalyticsEngine::new(db.clone())),
    });

    // Customize shop-custom and shop-blank via the real settings endpoint.
    let (status, login_custom) = post_login(
        &router,
        json!({ "username": "admin", "password": "custom-password-1", "shopCode": "shop-custom" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login_custom}");
    let custom_token = login_custom["data"]["token"]
        .as_str()
        .expect("token")
        .to_string();
    let (status, updated_custom) = send_authed(
        &router,
        "PUT",
        "/api/settings/shop-profile",
        &custom_token,
        Some(json!({ "tradingName": "Custom Trading Name", "allowSharedUserSessions": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated_custom}");

    let (status, login_blank) = post_login(
        &router,
        json!({ "username": "admin", "password": "blank-password-1", "shopCode": "shop-blank" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login_blank}");
    let blank_token = login_blank["data"]["token"]
        .as_str()
        .expect("token")
        .to_string();
    let (status, updated_blank) = send_authed(
        &router,
        "PUT",
        "/api/settings/shop-profile",
        &blank_token,
        Some(json!({ "allowSharedUserSessions": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated_blank}");
    assert_eq!(updated_blank["data"]["tradingName"], "");

    // Zero runtime fallback: before backfill, Staff login for legacy shop fails cleanly (500).
    let (pre_status, _) = post_login(
        &router,
        json!({ "username": "cashier1", "password": "staff-password-1", "shopCode": "shop-legacy" }),
    )
    .await;
    assert_eq!(pre_status, StatusCode::INTERNAL_SERVER_ERROR);

    // Run startup backfill (twice to verify idempotency).
    let first_count = ensure_existing_tenants_data(&db)
        .await
        .expect("backfill existing tenants");
    assert_eq!(first_count, 3);
    let second_count = ensure_existing_tenants_data(&db)
        .await
        .expect("idempotent backfill rerun");
    assert_eq!(second_count, 3);

    // Staff login on previously-missing legacy shop now succeeds.
    let (post_status, post_body) = post_login(
        &router,
        json!({ "username": "cashier1", "password": "staff-password-1", "shopCode": "shop-legacy" }),
    )
    .await;
    assert_eq!(post_status, StatusCode::OK, "{post_body}");
    assert_eq!(post_body["data"]["user"]["role"], "staff");

    // Verify backfilled legacy shop profile and drawer defaults.
    let legacy_profile = raw
        .collection::<Document>("shop_profiles")
        .find_one(doc! { "tenant_id": &legacy.key })
        .await
        .expect("query legacy shop profile")
        .expect("legacy shop profile must exist after backfill");
    assert_eq!(
        legacy_profile.get_str("trading_name").ok(),
        Some("Legacy Electronics")
    );
    assert_eq!(
        legacy_profile.get_bool("allow_shared_user_sessions").ok(),
        Some(false)
    );
    assert_eq!(
        legacy_profile
            .get_bool("allow_staff_multi_device_sessions")
            .ok(),
        Some(false)
    );
    assert_eq!(
        raw.collection::<Document>("cash_settings")
            .count_documents(doc! { "tenant_id": &legacy.key })
            .await
            .expect("count legacy cash settings"),
        1
    );
    assert!(
        raw.collection::<Document>("cash_categories")
            .count_documents(doc! { "tenant_id": &legacy.key })
            .await
            .expect("count legacy cash categories")
            > 0
    );

    // Custom shop profile edits remain completely intact.
    let (status, fetched_custom) = send_authed(
        &router,
        "GET",
        "/api/settings/shop-profile",
        &custom_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fetched_custom}");
    assert_eq!(fetched_custom["data"]["tradingName"], "Custom Trading Name");
    assert_eq!(fetched_custom["data"]["allowSharedUserSessions"], true);
    assert_eq!(
        fetched_custom["data"]["allowStaffMultiDeviceSessions"],
        false
    );

    // Blank profile gets its trading name backfilled without losing shared-shift preference.
    let (status, fetched_blank) = send_authed(
        &router,
        "GET",
        "/api/settings/shop-profile",
        &blank_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fetched_blank}");
    assert_eq!(fetched_blank["data"]["tradingName"], "Blank Profile Name");
    assert_eq!(fetched_blank["data"]["allowSharedUserSessions"], true);
    assert_eq!(
        fetched_blank["data"]["allowStaffMultiDeviceSessions"],
        false
    );

    // Zero "__deny__" documents anywhere after startup backfill.
    for collection in ["shop_profiles", "cash_settings", "cash_categories"] {
        let deny_count = raw
            .collection::<Document>(collection)
            .count_documents(doc! { "tenant_id": DENY_TENANT })
            .await
            .expect("count deny docs");
        assert_eq!(
            deny_count, 0,
            "{collection} must have zero {DENY_TENANT} rows"
        );
    }

    raw.drop().await.ok();
}
