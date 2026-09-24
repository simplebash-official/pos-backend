mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use simplebash_pos_backend::{
    app::{self, AppState},
    clients::{self, db::Db},
    core::{
        config::{Config, DatabaseType},
        constants::roles,
    },
    domain::users::Role,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct SqliteTestContext {
    router: axum::Router,
    config: Arc<Config>,
    db_path: std::path::PathBuf,
    pool: sqlx::SqlitePool,
}

impl Drop for SqliteTestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let shm = self.db_path.with_extension("db-shm");
        let wal = self.db_path.with_extension("db-wal");
        let _ = std::fs::remove_file(shm);
        let _ = std::fs::remove_file(wal);
    }
}

async fn setup_sqlite_app() -> SqliteTestContext {
    dotenvy::dotenv().ok();
    let db_path = std::env::temp_dir().join(format!("pos_backup_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = clients::sqlite::connect(&db_url)
        .await
        .expect("failed to initialize SQLite test database");

    let mock_doc_server_url = common::start_mock_document_server().await;

    let config = Config {
        database_type: DatabaseType::Sqlite,
        database_url: db_url,
        mongodb_uri: String::new(),
        mongodb_db_name: String::new(),
        jwt_secret: "test-sqlite-jwt-secret-key-that-is-long-enough".to_string(),
        port: 8080,
        bind_addr: "127.0.0.1".to_string(),
        jwt_expiry_hours: 12,
        document_server_url: mock_doc_server_url,
        document_server_api_key: "test-doc-server-api-key".to_string(),
        generated_documents_dir: std::env::temp_dir()
            .join(format!("docs_{}", Uuid::new_v4()))
            .to_string_lossy()
            .into_owned(),
        return_window_days: 30,
        auto_seed: false,
        tenant_mode: simplebash_pos_backend::core::config::TenantMode::Single,
        cors_allowed_origins: Vec::new(),
        identity_jwks_url: None,
        identity_issuer: None,
        provision_secret: None,
    };

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));

    let db = Db::Sqlite(pool.clone());
    let reports_engine =
        Arc::new(simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()));

    let state = AppState {
        config: config.clone(),
        db,
        document_server,
        reports_engine,
    };

    let router = app::build_router(state);

    SqliteTestContext {
        router,
        config,
        db_path,
        pool,
    }
}

#[tokio::test]
async fn export_backup_authorization_gates() {
    let ctx = setup_sqlite_app().await;

    // 1. Unauthenticated request returns 401 Unauthorized
    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/export")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Staff user token returns 403 Forbidden
    let staff_perms = roles::default_permissions(Role::Staff);
    let staff_token = common::mint_token(&ctx.config, Some(Role::Staff), staff_perms);
    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/export")
                .header(AUTHORIZATION, format!("Bearer {staff_token}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 3. Admin user token returns 200 OK
    let admin_perms = roles::default_permissions(Role::Admin);
    let admin_token = common::mint_token(&ctx.config, Some(Role::Admin), admin_perms);
    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/export")
                .header(AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "includeSettings": true }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn export_and_restore_backup_roundtrip() {
    let ctx = setup_sqlite_app().await;
    let admin_perms = roles::default_permissions(Role::Admin);
    let admin_token = common::mint_token(&ctx.config, Some(Role::Admin), admin_perms);

    // Insert sample category and product directly into SQLite
    sqlx::query(
        "INSERT INTO categories (key, id, name, icon, color, version, created_at, updated_at) \
         VALUES ('cat_1', '65f1a1a1a1a1a1a1a1a1a1a1', 'Laptops', 'laptop', '#3b82f6', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&ctx.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO subcategories (key, id, category_key, name, version, created_at, updated_at) \
         VALUES ('subcat_1', '65f1a1a1a1a1a1a1a1a1a1a2', 'cat_1', 'Gaming', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&ctx.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO products (key, id, sku, name, category_key, subcategory_key, cost_price_cents, selling_price_cents, stock_quantity, version, created_at, updated_at) \
         VALUES ('prod_1', '65f1a1a1a1a1a1a1a1a1a1a3', 'LAP-GAM-0001', 'Asus ROG', 'cat_1', 'subcat_1', 150000, 200000, 5, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&ctx.pool)
    .await
    .unwrap();

    // 1. Export backup with client settings
    let client_settings = json!({
        "shopProfile": { "tradingName": "Backup Test Shop", "primaryPhone": "0771234567" },
        "printSettings": { "receiptWidth": "80mm" }
    });

    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/export")
                .header(AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "includeSettings": true,
                        "settings": client_settings
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let export_res: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(export_res["success"].as_bool().unwrap());
    let backup_data = export_res["data"].clone();

    // Verify backup contains our product and settings
    assert_eq!(
        backup_data["settings"]["shopProfile"]["tradingName"],
        "Backup Test Shop"
    );
    let products_array = backup_data["tables"]["products"].as_array().unwrap();
    assert_eq!(products_array.len(), 1);
    assert_eq!(products_array[0]["name"], "Asus ROG");

    // 2. Clear database to simulate data loss
    sqlx::query("DELETE FROM products")
        .execute(&ctx.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM subcategories")
        .execute(&ctx.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM categories")
        .execute(&ctx.pool)
        .await
        .unwrap();

    // Verify table is empty
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM products")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);

    // 3. Restore backup
    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/import")
                .header(AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "backup": backup_data }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let restore_res: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(restore_res["success"].as_bool().unwrap());
    assert_eq!(
        restore_res["data"]["settings"]["shopProfile"]["tradingName"],
        "Backup Test Shop"
    );

    // 4. Verify product, category, and subcategory are fully restored in SQLite
    let restored_prod: (String, String) =
        sqlx::query_as("SELECT key, name FROM products WHERE key = 'prod_1'")
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(restored_prod.1, "Asus ROG");
}

#[tokio::test]
async fn restore_rejects_incompatible_version() {
    let ctx = setup_sqlite_app().await;
    let admin_perms = roles::default_permissions(Role::Admin);
    let admin_token = common::mint_token(&ctx.config, Some(Role::Admin), admin_perms);

    let invalid_backup = json!({
        "version": 999, // Incompatible version
        "exportedAt": "2026-01-01T00:00:00Z",
        "appVersion": "0.1.0",
        "environment": "desktop",
        "totalTables": 0,
        "totalRecords": 0,
        "tables": {}
    });

    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/import")
                .header(AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "backup": invalid_backup }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restore_handles_future_schema_evolution_tables_and_columns() {
    let ctx = setup_sqlite_app().await;
    let admin_perms = roles::default_permissions(Role::Admin);
    let admin_token = common::mint_token(&ctx.config, Some(Role::Admin), admin_perms);

    // Create a backup that contains:
    // 1. A table that does not exist in the current database (`deprecated_future_table`)
    // 2. A column in `categories` that does not exist (`obsolete_removed_column`)
    let evolving_backup = json!({
        "version": 1,
        "exportedAt": "2026-01-01T00:00:00Z",
        "appVersion": "0.1.0",
        "environment": "desktop",
        "totalTables": 2,
        "totalRecords": 2,
        "tables": {
            "categories": [
                {
                    "key": "cat_evolve_1",
                    "id": "65f1a1a1a1a1a1a1a1a1a1a1",
                    "name": "Evolving Category",
                    "icon": "box",
                    "color": "#10b981",
                    "version": 1,
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "obsolete_removed_column": "should be ignored safely"
                }
            ],
            "deprecated_future_table": [
                {
                    "some_key": "val_1",
                    "some_data": 123
                }
            ]
        }
    });

    let res = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/import")
                .header(AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "backup": evolving_backup }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let restore_res: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(restore_res["success"].as_bool().unwrap());

    // Verify category was restored successfully and the nonexistent table was skipped
    let restored_cat: (String, String) =
        sqlx::query_as("SELECT key, name FROM categories WHERE key = 'cat_evolve_1'")
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(restored_cat.1, "Evolving Category");
}
