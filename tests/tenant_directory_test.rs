// Tenant directory + multi-tenant login, against a real MongoDB (MONGODB_URI)
// in a uniquely named throwaway database that is dropped afterwards. Both
// tenants deliberately use the SAME admin email with different passwords, so a
// login that ignored the shop code (or leaked across tenants) would succeed
// where it must not.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    app::{self, AppState},
    clients::{
        self, db::Db, document_server::DocumentServerClient, mongo::connect,
        tenant_db::TenantDatabase,
    },
    core::{
        config::{Config, DatabaseType, TenantMode},
        middleware::auth::Claims,
        tenancy::{Tenant, with_tenant},
    },
    domain::users::{CreateUserRequest, Role},
    modules::{
        reports::engine::AnalyticsEngine,
        tenants::service::{create_tenant, create_tenant_with_key, resolve_shop_code},
        users::service::create_user,
    },
};
use tower::ServiceExt;
use uuid::Uuid;

async fn post_login(router: &axum::Router, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn tenant_directory_and_multi_tenant_login_roundtrip() {
    dotenvy::dotenv().ok();
    let Ok(_) = std::env::var("MONGODB_URI") else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };

    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.database_type = DatabaseType::Mongo;
    config.tenant_mode = TenantMode::Multi;
    config.mongodb_db_name = format!("jtdir_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let raw = connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("connect throwaway db");
    clients::indexes::ensure_indexes(&raw, true).await;
    let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

    // --- directory ---------------------------------------------------------
    let a = create_tenant(&db, "  Shop-Alpha ", "Alpha Repairs").await.unwrap();
    let b = create_tenant(&db, "shop-beta", "Beta Retail").await.unwrap();
    assert_eq!(a.shop_code, "shop-alpha", "codes are lowercased and trimmed");
    assert!(a.key.starts_with("tnt_"));
    assert_ne!(a.key, b.key);

    let dup = create_tenant(&db, "SHOP-ALPHA", "Another").await.unwrap_err();
    assert!(format!("{dup:?}").contains("SHOP_CODE_ALREADY_EXISTS"), "{dup:?}");
    assert!(create_tenant(&db, "no", "Too short").await.is_err());

    let resolved = resolve_shop_code(&db, "Shop-Alpha").await.unwrap();
    assert_eq!(resolved, Tenant::id(&a.key).unwrap());
    // A tenant registered under the id the identity service issued keeps that id,
    // so a staff login by shop code reaches the data the devices sync.
    let issued = create_tenant_with_key(&db, "tnt_issuedbyidentity1".to_string(), "test-shop", "Test Shop")
        .await
        .unwrap();
    assert_eq!(issued.key, "tnt_issuedbyidentity1");
    assert_eq!(
        resolve_shop_code(&db, "test-shop").await.unwrap(),
        Tenant::id("tnt_issuedbyidentity1").unwrap()
    );
    for bad in ["issued", "tnt_", "tnt_has space"] {
        assert!(
            create_tenant_with_key(&db, bad.to_string(), "other-shop", "Other").await.is_err(),
            "{bad:?} must be rejected as a tenant id"
        );
    }

    let unknown = resolve_shop_code(&db, "nobody-here").await.unwrap_err();
    assert!(format!("{unknown:?}").contains("TENANT_NOT_FOUND"), "{unknown:?}");

    // --- same admin email in both tenants ------------------------------------
    for (info, name, password) in [
        (&a, "Alpha Admin", "alpha-password-1"),
        (&b, "Beta Admin", "beta-password-2"),
    ] {
        with_tenant(Tenant::id(&info.key).unwrap(), async {
            create_user(
                &db,
                CreateUserRequest {
                    name: name.to_string(),
                    email: "owner@example.test".to_string(),
                    password: password.to_string(),
                    role: Role::Admin,
                    employee_key: None,
                },
            )
            .await
            .expect("each tenant can own the same email")
        })
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
        .unwrap()
        .claims
    };

    let (status, body) = post_login(
        &router,
        json!({ "email": "owner@example.test", "password": "alpha-password-1", "shopCode": "shop-alpha" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token_a = body["data"]["token"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["user"]["name"], "Alpha Admin");
    assert_eq!(claims_of(&token_a).tid.as_deref(), Some(a.key.as_str()));

    let (status, body) = post_login(
        &router,
        json!({ "email": "owner@example.test", "password": "beta-password-2", "shopCode": "SHOP-BETA" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["user"]["name"], "Beta Admin");
    assert_eq!(
        claims_of(body["data"]["token"].as_str().unwrap()).tid.as_deref(),
        Some(b.key.as_str())
    );

    // Alpha's password does not work in Beta's shop, and vice versa.
    let (status, _) = post_login(
        &router,
        json!({ "email": "owner@example.test", "password": "alpha-password-1", "shopCode": "shop-beta" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Unknown shop looks exactly like a wrong password; a missing code is a 400.
    let (status, _) = post_login(
        &router,
        json!({ "email": "owner@example.test", "password": "alpha-password-1", "shopCode": "ghost-shop" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = post_login(
        &router,
        json!({ "email": "owner@example.test", "password": "alpha-password-1" }),
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
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(me.into_body(), usize::MAX).await.unwrap();
    let me: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["data"]["name"], "Alpha Admin");

    raw.drop().await.ok();
}

#[tokio::test]
async fn multi_tenant_cors_only_allows_configured_origins() {
    dotenvy::dotenv().ok();
    let Ok(_) = std::env::var("MONGODB_URI") else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };
    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.database_type = DatabaseType::Mongo;
    config.tenant_mode = TenantMode::Multi;
    config.mongodb_db_name = format!("jtdir_{}", &Uuid::new_v4().simple().to_string()[..24]);
    config.cors_allowed_origins = vec!["https://app.example.test".to_string()];
    let raw = connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("connect throwaway db");
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
                        .unwrap(),
                )
                .await
                .unwrap();
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
