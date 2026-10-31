//! Documents and migrations tests.

use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use simplebash_pos_backend::clients;

use uuid::Uuid;

use crate::helpers::{admin_token, execute, send_authed, setup_sqlite_app};

#[tokio::test]
async fn test_sqlite_generated_documents_schema_migration() {
    dotenvy::dotenv().ok();
    let db_path =
        std::env::temp_dir().join(format!("pos_gen_doc_migration_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let options: sqlx::sqlite::SqliteConnectOptions = db_url.parse().expect("the text parses");
    let options = options.create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options)
        .await
        .expect("the awaited step succeeds");

    // 1. Artificially create legacy generated_documents schema missing entity_key, document_type, file_size_bytes
    sqlx::query(
        r#"
        CREATE TABLE generated_documents (
            key TEXT PRIMARY KEY,
            id TEXT NOT NULL,
            template_key TEXT NOT NULL,
            template_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .expect("the awaited step succeeds");

    // 2. Run init_db which must automatically heal and add missing columns & index
    clients::sqlite::init_db(&pool)
        .await
        .expect("the awaited step succeeds");

    // 3. Verify entity_key, document_type, and file_size_bytes are added
    let columns: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('generated_documents')")
            .fetch_all(&pool)
            .await
            .expect("the awaited step succeeds");

    assert!(
        columns.iter().any(|(_, name)| name == "entity_key"),
        "entity_key column should have been added by migrate_schema"
    );
    assert!(
        columns.iter().any(|(_, name)| name == "document_type"),
        "document_type column should have been added by migrate_schema"
    );
    assert!(
        columns.iter().any(|(_, name)| name == "file_size_bytes"),
        "file_size_bytes column should have been added by migrate_schema"
    );

    // 4. Test SELECT query with all columns works without database error
    let row_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM generated_documents
        WHERE entity_key = 'inv_123' AND document_type = 'a4-invoice'
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("SELECT on generated_documents with entity_key must succeed");
    assert_eq!(row_count, 0);

    // 5. Test INSERT into generated_documents with all 10 columns succeeds
    sqlx::query(
        r#"
        INSERT INTO generated_documents (
            key, id, entity_key, document_type, template_name, template_key,
            file_path, file_size_bytes, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("doc_test123456789")
    .bind("507f1f77bcf86cd799439011")
    .bind("inv_test123456789")
    .bind("a4-invoice")
    .bind("a4-invoice")
    .bind("tpl_a4_invoice")
    .bind("inv_test123456789/a4-invoice.pdf")
    .bind(1024_i64)
    .bind("2026-09-06T10:00:00Z")
    .bind("2026-09-06T10:00:00Z")
    .execute(&pool)
    .await
    .expect("INSERT INTO generated_documents must succeed after migration");

    // Clean up temp database
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_repeated_failed_logins_are_throttled() {
    let ctx = setup_sqlite_app().await;
    // Unique per run: the limiter is process-wide, so a shared username could be
    // pushed over the limit by other tests' failures.
    let username = format!("brute-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let attempt = || {
        Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "username": username, "password": "wrong-password-123" }).to_string(),
            ))
            .expect("the request is well formed")
    };

    for i in 0..10 {
        let (status, body) = execute(&ctx.router, attempt()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {i}: {body}");
    }
    let (status, body) = execute(&ctx.router, attempt()).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "TOO_MANY_LOGIN_ATTEMPTS");
}

#[tokio::test]
async fn test_sqlite_deactivating_a_user_revokes_their_live_token() {
    let ctx = setup_sqlite_app().await;
    let admin = admin_token(&ctx.config);
    let username = format!("staff-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let password = "Staff-Password-123";

    let (status, body) = send_authed(
        &ctx.router,
        "POST",
        "/api/users",
        Some(json!({ "name": "Staff Member", "username": username, "password": password, "role": "staff" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let user_id = body["data"]["id"].as_str().expect("user id").to_string();

    let login = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "username": username, "password": password }).to_string(),
        ))
        .expect("the request is well formed");
    let (status, body) = execute(&ctx.router, login).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let staff_token = body["data"]["token"].as_str().expect("token").to_string();

    let (status, body) =
        send_authed(&ctx.router, "GET", "/api/customers", None, &staff_token).await;
    assert_eq!(status, StatusCode::OK, "before deactivation: {body}");

    let (status, body) = send_authed(
        &ctx.router,
        "PATCH",
        &format!("/api/users/{user_id}"),
        Some(json!({ "isActive": false })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The token itself is still unexpired, but the account behind it is not
    // active any more: every authenticated route must refuse it now.
    let (status, body) =
        send_authed(&ctx.router, "GET", "/api/customers", None, &staff_token).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "after deactivation: {body}"
    );
    assert_eq!(body["code"], "USER_INACTIVE");
}

#[tokio::test]
async fn test_sqlite_product_sort_is_stable_and_preferences_are_per_login() {
    let ctx = setup_sqlite_app().await;
    let admin = admin_token(&ctx.config);

    let (_, cat) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Sorting", "icon": "devices", "color": "blue" })),
        &admin,
    )
    .await;
    let category_key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (_, sub) = send_authed(
        &ctx.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Items" })),
        &admin,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let mut ids = std::collections::HashMap::new();
    for (name, price) in [("banana", 300), ("Apple", 200), ("cherry", 200)] {
        let (status, res) = send_authed(
            &ctx.router,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": name,
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "openingStock": { "quantity": 5, "unitCostCents": 100, "sellingPriceCents": price },
                "minStockThreshold": 1
            })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        ids.insert(
            name,
            res["data"]["id"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }

    let names = |body: &Value| -> Vec<String> {
        body["data"]["items"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .map(|p| {
                p["name"]
                    .as_str()
                    .expect("the value is a string")
                    .to_string()
            })
            .collect()
    };

    // Default: name A-Z, case-insensitive, unaffected by editing a product.
    let (status, _) = send_authed(
        &ctx.router,
        "PUT",
        &format!("/api/inventory/products/{}", ids["cherry"]),
        Some(json!({ "minStockThreshold": 2 })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send_authed(
        &ctx.router,
        "GET",
        "/api/inventory/products?limit=50",
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(names(&body), ["Apple", "banana", "cherry"]);

    let (_, body) = send_authed(
        &ctx.router,
        "GET",
        "/api/inventory/products?limit=50&sortBy=sellingPriceCents&sortOrder=asc",
        None,
        &admin,
    )
    .await;
    assert_eq!(names(&body), ["Apple", "cherry", "banana"]);

    // Preferences: stored per login on SQLite, independent between logins.
    let mut tokens = Vec::new();
    for label in ["a", "b"] {
        let username = format!(
            "pref-{label}-{}",
            &Uuid::new_v4().simple().to_string()[..10]
        );
        let (status, body) = send_authed(
            &ctx.router,
            "POST",
            "/api/users",
            Some(json!({ "name": "Pref User", "username": username, "password": "Staff-Password-123", "role": "staff" })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let login = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "username": username, "password": "Staff-Password-123" }).to_string(),
            ))
            .expect("the request is well formed");
        let (status, body) = execute(&ctx.router, login).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        tokens.push(
            body["data"]["token"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }

    let (status, body) = send_authed(
        &ctx.router,
        "PATCH",
        "/api/auth/me/preferences",
        Some(json!({ "billingCatalogSort": "price_desc" })),
        &tokens[0],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, me_a) = send_authed(&ctx.router, "GET", "/api/auth/me", None, &tokens[0]).await;
    assert_eq!(
        me_a["data"]["preferences"]["billingCatalogSort"],
        "price_desc"
    );
    let (_, me_b) = send_authed(&ctx.router, "GET", "/api/auth/me", None, &tokens[1]).await;
    assert!(me_b["data"]["preferences"]["billingCatalogSort"].is_null());

    let (status, _) = send_authed(
        &ctx.router,
        "PATCH",
        "/api/auth/me/preferences",
        Some(json!({ "billingCatalogSort": "nope" })),
        &tokens[0],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
