mod common;

use axum::body::Body;

use axum::http::{Request, StatusCode, header};
use serde_json::json;
use tower::ServiceExt;

use simplebash_pos_backend::domain::users::Role;

async fn send_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }

    let request = if let Some(json_body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json_body).expect("the value serializes to JSON"),
            ))
            .expect("the value serializes to JSON")
    } else {
        builder
            .body(Body::empty())
            .expect("the value serializes to JSON")
    };

    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");

    let json: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null)
    };

    (status, json)
}

#[tokio::test]
async fn test_shop_profile_initial_defaults_are_empty_in_sqlite() {
    let app = common::spawn_app_sqlite().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    let (status, res) = send_request(
        &app.router,
        "GET",
        "/api/settings/shop-profile",
        Some(&token),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["success"], true);
    let data = &res["data"];
    assert_eq!(data["legalName"], "");
    assert_eq!(data["tradingName"], "");
    assert_eq!(data["addressLines"], json!([]));
    assert_eq!(data["primaryPhone"], "");
    assert_eq!(data["secondaryPhone"], "");
    assert_eq!(data["email"], "");
    assert_eq!(data["website"], "");
    assert_eq!(data["version"], 1);
}

#[tokio::test]
async fn test_shop_profile_update_and_persistence() {
    let app = common::spawn_app_sqlite().await;
    let admin_token = common::mint_token(&app.config, Some(Role::Admin), &[]);
    let cashier_token = common::mint_token(&app.config, Some(Role::Staff), &[]);

    // 1. Update shop profile as Admin
    let update_body = json!({
        "legalName": "Acme Retail Ltd",
        "tradingName": "Shop 55",
        "addressLines": ["No. 42, Galle Road", "Colombo 03"],
        "primaryPhone": "0771234567",
        "secondaryPhone": "0112345678",
        "email": "contact@shop55.lk",
        "website": "https://shop55.lk"
    });

    let (status, update_res) = send_request(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&admin_token),
        Some(update_body),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_res["success"], true);
    let updated = &update_res["data"];
    assert_eq!(updated["legalName"], "Acme Retail Ltd");
    assert_eq!(updated["tradingName"], "Shop 55");
    assert_eq!(
        updated["addressLines"],
        json!(["No. 42, Galle Road", "Colombo 03"])
    );
    assert_eq!(updated["primaryPhone"], "0771234567");
    assert_eq!(updated["secondaryPhone"], "0112345678");
    assert_eq!(updated["email"], "contact@shop55.lk");
    assert_eq!(updated["website"], "https://shop55.lk");

    // 2. Fetch as Cashier — verify persistence
    let (get_status, get_res) = send_request(
        &app.router,
        "GET",
        "/api/settings/shop-profile",
        Some(&cashier_token),
        None,
    )
    .await;

    assert_eq!(get_status, StatusCode::OK);
    let data = &get_res["data"];
    assert_eq!(data["legalName"], "Acme Retail Ltd");
    assert_eq!(data["tradingName"], "Shop 55");
    assert_eq!(
        data["addressLines"],
        json!(["No. 42, Galle Road", "Colombo 03"])
    );
    assert_eq!(data["primaryPhone"], "0771234567");
    assert_eq!(data["secondaryPhone"], "0112345678");
    assert_eq!(data["email"], "contact@shop55.lk");
    assert_eq!(data["website"], "https://shop55.lk");

    // 3. Cashier cannot update shop profile
    let (fail_status, _) = send_request(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&cashier_token),
        Some(json!({ "tradingName": "Hacked Shop" })),
    )
    .await;
    assert_eq!(fail_status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_shop_profile_empty_trading_name_rejected() {
    let app = common::spawn_app_sqlite().await;
    let admin_token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    let (status, res) = send_request(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&admin_token),
        Some(json!({ "tradingName": "   " })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(res["success"], false);
}

#[tokio::test]
async fn test_shop_profile_errors_when_uninitialized_and_ensure_is_idempotent() {
    use tower::ServiceExt as _;

    let app = common::spawn_app_sqlite().await;
    if let simplebash_pos_backend::clients::db::Db::Sqlite(pool) = &app.db_handle {
        sqlx::query("DELETE FROM shop_profiles")
            .execute(pool)
            .await
            .expect("delete shop_profiles row");
    }

    let admin_token = common::mint_token(
        &app.config,
        Some(simplebash_pos_backend::domain::users::Role::Admin),
        simplebash_pos_backend::core::constants::roles::default_permissions(
            simplebash_pos_backend::domain::users::Role::Admin,
        ),
    );

    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/settings/shop-profile")
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {admin_token}"),
        )
        .body(axum::body::Body::empty())
        .expect("request builds");
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("oneshot succeeds");
    assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);

    simplebash_pos_backend::modules::settings::service::ensure_shop_profile(
        &app.db_handle,
        Some("First Trading Name"),
    )
    .await
    .expect("first ensure succeeds");
    simplebash_pos_backend::modules::settings::service::ensure_shop_profile(
        &app.db_handle,
        Some("Second Trading Name"),
    )
    .await
    .expect("second ensure is idempotent");

    let req2 = axum::http::Request::builder()
        .method("GET")
        .uri("/api/settings/shop-profile")
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {admin_token}"),
        )
        .body(axum::body::Body::empty())
        .expect("request builds");
    let resp2 = app
        .router
        .clone()
        .oneshot(req2)
        .await
        .expect("oneshot succeeds");
    assert_eq!(resp2.status(), axum::http::StatusCode::OK);
    let bytes = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .expect("body reads");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
    assert_eq!(json["data"]["tradingName"], "First Trading Name");
    assert_eq!(json["data"]["allowSharedUserSessions"], false);
    assert_eq!(json["data"]["allowStaffMultiDeviceSessions"], false);
}

#[tokio::test]
async fn sqlite_migration_adds_allow_staff_multi_device_sessions_column() {
    use simplebash_pos_backend::clients::db::Db;
    use simplebash_pos_backend::clients::sqlite::migrate_schema;
    use simplebash_pos_backend::domain::settings::ShiftMode;
    use simplebash_pos_backend::domain::settings::ShiftPolicy;
    use simplebash_pos_backend::domain::settings::StaffDevicePolicy;
    use simplebash_pos_backend::modules::sessions::service::current_shift_policy;
    use sqlx::Row;
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite opens");
    sqlx::raw_sql(
        r#"
        CREATE TABLE shop_profiles (
            key TEXT PRIMARY KEY,
            id TEXT NOT NULL,
            legal_name TEXT NOT NULL DEFAULT '',
            trading_name TEXT NOT NULL DEFAULT '',
            address_lines TEXT NOT NULL DEFAULT '[]',
            primary_phone TEXT NOT NULL DEFAULT '',
            secondary_phone TEXT NOT NULL DEFAULT '',
            email TEXT NOT NULL DEFAULT '',
            website TEXT NOT NULL DEFAULT '',
            business_reg_no TEXT NOT NULL DEFAULT '',
            logo_base64 TEXT NOT NULL DEFAULT '',
            bank_name TEXT NOT NULL DEFAULT '',
            bank_branch TEXT NOT NULL DEFAULT '',
            account_name TEXT NOT NULL DEFAULT '',
            account_number TEXT NOT NULL DEFAULT '',
            default_warranty_text TEXT NOT NULL DEFAULT '',
            default_footer_text TEXT NOT NULL DEFAULT '',
            receipt_footer_text TEXT NOT NULL DEFAULT '',
            allow_shared_user_sessions INTEGER NOT NULL DEFAULT 0,
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            updated_by_device TEXT
        );
        INSERT INTO shop_profiles (
            key, id, trading_name, allow_shared_user_sessions, version, created_at, updated_at
        ) VALUES (
            'main', '0123456789abcdef01234567', 'Legacy SQLite Shop', 0, 1,
            '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
        );
        "#,
    )
    .execute(&pool)
    .await
    .expect("legacy shop_profiles table created");

    migrate_schema(&pool)
        .await
        .expect("first migration succeeds");
    migrate_schema(&pool)
        .await
        .expect("second migration is idempotent");

    let flag_int: i64 = sqlx::query(
        "SELECT allow_staff_multi_device_sessions FROM shop_profiles WHERE key = 'main'",
    )
    .fetch_one(&pool)
    .await
    .expect("migrated column queries")
    .get("allow_staff_multi_device_sessions");
    assert_eq!(flag_int, 0);

    let policy = current_shift_policy(&Db::Sqlite(pool))
        .await
        .expect("migrated row loads via current_shift_policy");
    assert_eq!(
        policy,
        ShiftPolicy {
            mode: ShiftMode::Personal,
            staff_device: StaffDevicePolicy::SingleDeviceWhileShiftOpen,
        }
    );
    assert!(!policy.allows_staff_multi_device());
}
