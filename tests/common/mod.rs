use std::sync::Arc;

use axum::Router;
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::Database;
use serde_json::json;
use simplebash_pos_backend::{
    app, app::AppState, clients, core::config::Config, core::middleware::auth::Claims,
    domain::users::Role,
};

pub struct TestApp {
    #[allow(dead_code)]
    pub router: Router,
    // Not every integration test binary that includes this module needs
    // direct DB access (e.g. to seed data outside the API) or the config
    // (e.g. to mint a JWT with `jwt_secret`) — these fields are dead code
    // from the perspective of whichever one doesn't.
    #[allow(dead_code)]
    pub db: Database,
    #[allow(dead_code)]
    pub db_handle: simplebash_pos_backend::clients::db::Db,
    #[allow(dead_code)]
    pub config: Arc<Config>,
}

/// Spawns a lightweight in-process mock document-server on a free port so
/// integration tests can exercise document rendering endpoints without
/// requiring a live external document-server process.
///
/// Serves the same minimal `dataSchema` contracts the real service publishes
/// for its three billing-relevant templates — enough for the client's
/// pre-validation to run for real against realistic payloads.
pub async fn start_mock_document_server() -> String {
    start_mock_document_server_with_templates(mock_templates_json()).await
}

/// Same mock, but publishing a deliberately unsatisfiable schema for the
/// A4 Invoice (requires a field no payload builder will ever send). Used
/// by the pre-validation failure-path test, which must not disturb the
/// shared default mock other tests rely on.
#[allow(dead_code)]
pub async fn start_strict_mock_document_server() -> String {
    let mut templates = mock_templates_json();
    for template in templates["data"]["templates"]
        .as_array_mut()
        .expect("default mock template list")
        .iter_mut()
    {
        if template["description"] == "A4 Invoice" {
            template["dataSchema"]["required"] =
                json!(["invoiceNumber", "__never_sent_by_backend"]);
        }
    }
    start_mock_document_server_with_templates(templates).await
}

/// One template entry as `GET /api/templates` publishes it (the latest
/// published version of a design family). `kind` goes into the schema as
/// `x-document-kind`; a bundled template is its own family.
fn mock_template(
    key: &str,
    name: &str,
    title: &str,
    kind: &str,
    contract: serde_json::Value,
    is_default: bool,
    sample: serde_json::Value,
) -> serde_json::Value {
    json!({
        "key": key,
        "name": name,
        "family": name,
        "version": 1,
        "type": "document",
        "kind": kind,
        "description": title,
        "status": "published",
        "scope": "global",
        "isDefault": is_default,
        "options": [],
        "sha256": "0".repeat(64),
        "minAppVersion": null,
        "dataSchema": {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": title,
            "x-document-kind": kind,
            "type": "object",
            "additionalProperties": true,
            "required": contract["required"],
            "properties": contract["properties"]
        },
        "data": sample,
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z"
    })
}

/// A published custom design: a private package of the A4 invoice kind whose
/// schema requires the `options` object its two controls fill.
pub fn mock_private_a4_invoice() -> serde_json::Value {
    let mut template = mock_template(
        "tpl_client_invoice",
        "doc_temp_ClientInvoiceV2",
        "Client Invoice",
        "a4-invoice",
        json!({
            "required": ["invoiceNumber", "options"],
            "properties": {
                "invoiceNumber": { "type": "string" },
                "options": {
                    "type": "object",
                    "required": ["showSign", "accent"],
                    "properties": {
                        "showSign": { "type": "boolean" },
                        "accent": { "type": "string" }
                    }
                }
            }
        }),
        false,
        json!({ "invoiceNumber": "INV-CLIENT", "options": { "showSign": true, "accent": "blue" } }),
    );
    template["family"] = json!("fam_ClientInvoice00");
    template["version"] = json!(2);
    template["scope"] = json!("private");
    template["sha256"] = json!("a".repeat(64));
    template["options"] = json!([
        { "key": "showSign", "type": "toggle", "label": "Signature box", "default": true },
        {
            "key": "accent", "type": "select", "label": "Accent colour", "default": "blue",
            "choices": [{ "value": "blue", "label": "Blue" }, { "value": "red", "label": "Red" }]
        }
    ]);
    template
}

fn mock_templates_json() -> serde_json::Value {
    let a4 = json!({
        "required": ["invoiceNumber"],
        "properties": {
            "invoiceNumber": { "type": "string" },
            "logoUrl": { "type": "string" }
        }
    });
    let receipt = json!({
        "required": ["paperWidthMm", "invoiceNumber"],
        "properties": {
            "paperWidthMm": { "type": "integer", "enum": [58, 80] },
            "invoiceNumber": { "type": "string" }
        }
    });
    let sample_logo = "https://example.invalid/logo.png";
    // The label template is typed `label`, like the real one.
    let mut product_label = mock_template(
        "tpl_product_label",
        "lbl_temp_qklcWIolwoFFN3xk",
        "Product Sticker Label",
        "product-label",
        json!({
            "required": ["widthMm", "heightMm", "shopName", "elements", "labels"],
            "properties": {
                "widthMm": { "type": "number" },
                "heightMm": { "type": "number" },
                "shopName": { "type": "string" },
                "elements": { "type": "array", "minItems": 1 },
                "labels": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "required": ["name", "sku", "category", "priceCents", "code", "copies"],
                        "properties": {
                            "priceCents": { "type": ["integer", "null"] },
                            "code": { "type": "string", "minLength": 1 },
                            "copies": { "type": "integer", "minimum": 1 }
                        }
                    }
                }
            }
        }),
        true,
        json!({}),
    );
    product_label["type"] = json!("label");
    json!({
        "success": true,
        "message": "Templates fetched",
        "data": {
            "templates": [
                mock_template(
                    "tpl_a4_invoice",
                    "doc_temp_vEf0Y7jQHQj2rIuO",
                    "A4 Invoice",
                    "a4-invoice",
                    a4.clone(),
                    true,
                    json!({ "invoiceNumber": "INV-CLASSIC", "logoUrl": sample_logo }),
                ),
                mock_template(
                    "tpl_a4_invoice_modern",
                    "doc_temp_ZahcoYg8xCQjuc8c",
                    "A4 Invoice Modern",
                    "a4-invoice",
                    a4,
                    false,
                    json!({ "invoiceNumber": "INV-MODERN", "logoUrl": sample_logo }),
                ),
                mock_private_a4_invoice(),
                mock_template(
                    "tpl_thermal_receipt",
                    "doc_temp_4pz79z5iba7TcEIp",
                    "Thermal Receipt",
                    "thermal-receipt",
                    receipt.clone(),
                    true,
                    json!({ "paperWidthMm": 80, "invoiceNumber": "INV-RECEIPT" }),
                ),
                mock_template(
                    "tpl_thermal_receipt_compact",
                    "doc_temp_CompactReceipt00",
                    "Compact Receipt",
                    "thermal-receipt",
                    receipt,
                    false,
                    json!({ "paperWidthMm": 58, "invoiceNumber": "INV-COMPACT" }),
                ),
                mock_template(
                    "tpl_credit_note",
                    "doc_temp_5DHl8hUQTX3oLBSR",
                    "Credit Note",
                    "credit-note",
                    json!({
                        "required": ["creditNoteNumber"],
                        "properties": { "creditNoteNumber": { "type": "string" } }
                    }),
                    true,
                    json!({ "creditNoteNumber": "CN-SAMPLE" }),
                ),
                mock_template(
                    "tpl_credit_note_compact",
                    "doc_temp_CompactCredit000",
                    "Credit Note Compact",
                    "credit-note",
                    json!({
                        "required": ["creditNoteNumber"],
                        "properties": { "creditNoteNumber": { "type": "string" } }
                    }),
                    false,
                    json!({ "creditNoteNumber": "CN-COMPACT" }),
                ),
                mock_template(
                    "tpl_quotation",
                    "doc_temp_Qu0tati0nV1Std00",
                    "Quotation",
                    "quotation",
                    json!({
                        "required": ["quotationNumber", "validUntil", "items"],
                        "properties": {
                            "quotationNumber": { "type": "string" },
                            "validUntil": { "type": "string" },
                            "items": { "type": "array" }
                        }
                    }),
                    true,
                    json!({ "quotationNumber": "QUO-SAMPLE", "validUntil": "01 Jan 2030", "items": [] }),
                ),
                product_label,
                mock_template(
                    "tpl_analytics_report",
                    "doc_temp_An1yT1csRep0rtV1",
                    "Analytics Report",
                    "analytics-report",
                    json!({
                        "required": ["generatedAt", "periodLabel", "granularityLabel", "kpis", "timeseries"],
                        "properties": {
                            "generatedAt": { "type": "string" },
                            "periodLabel": { "type": "string" },
                            "granularityLabel": { "type": "string" },
                            "kpis": { "type": "array" },
                            "timeseries": { "type": "object" }
                        }
                    }),
                    true,
                    json!({}),
                ),
                mock_template(
                    "tpl_z_report",
                    "doc_temp_ZRep0rtV1Std000",
                    "Z-Report",
                    "z-report",
                    json!({
                        "required": ["sessionNumber", "terminalId", "cashierName"],
                        "properties": {
                            "sessionNumber": { "type": "string" },
                            "terminalId": { "type": "string" },
                            "cashierName": { "type": "string" }
                        }
                    }),
                    true,
                    json!({ "sessionNumber": "SES-000042", "terminalId": "TERM-01", "cashierName": "Kasun Perera" }),
                ),
            ]
        }
    })
}

async fn start_mock_document_server_with_templates(
    templates_response: serde_json::Value,
) -> String {
    use axum::{
        Json,
        body::Body,
        extract::Path,
        http::{HeaderMap, StatusCode, header},
        response::IntoResponse,
        routing::{get, post},
    };

    let mock_app = Router::new()
        .route(
            "/api/health",
            get(|| async {
                Json(json!({
                    "success": true,
                    "message": "Service is healthy",
                    "data": { "status": "ok" }
                }))
            }),
        )
        .route(
            "/api/templates",
            get(move |headers: HeaderMap| {
                let templates_response = templates_response.clone();
                async move {
                    // Like the real service: the list needs the internal key.
                    if headers.get("X-Internal-Api-Key").is_none_or(|v| v.is_empty()) {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({
                                "success": false,
                                "code": "INTERNAL_API_KEY_INVALID",
                                "message": "missing api key"
                            })),
                        );
                    }
                    (StatusCode::OK, Json(templates_response))
                }
            }),
        )
        .route(
            "/api/packages/{name}/archive",
            get(|Path(name): Path<String>, headers: HeaderMap| async move {
                if headers.get("X-Internal-Api-Key").is_none_or(|v| v.is_empty()) {
                    return (StatusCode::UNAUTHORIZED, Body::empty()).into_response();
                }
                if name != "doc_temp_ClientInvoiceV2" {
                    return (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        Body::from(r#"{"success":false,"code":"PACKAGE_NOT_FOUND","message":"Package not found"}"#),
                    )
                        .into_response();
                }
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/zip")],
                    Body::from(format!("PK mock archive of {name}")),
                )
                    .into_response()
            }),
        )
        .route(
            "/api/render/{template_key}",
            post(
                |Path(template_key): Path<String>,
                 headers: HeaderMap,
                 body: axum::body::Bytes| async move {
                    if let Some(auth) = headers.get("X-Internal-Api-Key")
                        && auth.is_empty()
                    {
                        return (
                            StatusCode::UNAUTHORIZED,
                            [(header::CONTENT_TYPE, "application/json")],
                            Body::from(
                                r#"{"status":"error","code":"UNAUTHORIZED","message":"missing api key"}"#,
                            ),
                        );
                    }
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/pdf")],
                        Body::from(
                            // Tests read which template rendered, and whether the
                            // payload still carried a `logoUrl`, from this text.
                            format!(
                                "%PDF-1.7 mock rendered PDF for template {template_key} logoUrl={} body={}",
                                String::from_utf8_lossy(&body).contains("\"logoUrl\""),
                                String::from_utf8_lossy(&body)
                            )
                            .into_bytes(),
                        ),
                    )
                },
            ),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mock document server");
    let addr = listener.local_addr().expect("failed to get local addr");
    tokio::spawn(async move {
        axum::serve(listener, mock_app).await.ok();
    });

    format!("http://{}", addr)
}

/// Builds the real router against a Mongo test database and the default
/// mock document-server. Reads connection details from the environment
/// (`.env` is loaded, same as production) but always targets
/// `MONGODB_TEST_DB_NAME` so tests never touch dev data.
#[allow(dead_code)]
pub async fn spawn_app() -> TestApp {
    let mock_doc_server_url = start_mock_document_server().await;
    spawn_app_with_document_server_url(mock_doc_server_url).await
}

/// Same, but pointing the app at a caller-provided document-server URL —
/// used by tests that need a differently-configured mock (e.g. the strict-
/// schema variant for the pre-validation failure path).
#[allow(dead_code)]
pub async fn spawn_app_with_document_server_url(document_server_url: String) -> TestApp {
    dotenvy::dotenv().ok();
    let db_name =
        std::env::var("MONGODB_TEST_DB_NAME").unwrap_or_else(|_| "simplebash_pos_test".to_string());
    spawn_mongo_app(db_name, document_server_url).await
}

/// Single-tenant app against a uniquely named throwaway Atlas database
/// (`jtdocs_<uuid24>`, under Atlas's 38-byte cap). For tests that change
/// shop-wide settings (such as the chosen document design), which would
/// otherwise leak into every other test sharing `simplebash_pos_test`. The
/// caller drops the database when done (`app.db.drop()`).
#[allow(dead_code)]
pub async fn spawn_app_isolated() -> TestApp {
    dotenvy::dotenv().ok();
    let db_name = format!(
        "jtdocs_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..24]
    );
    spawn_mongo_app(db_name, start_mock_document_server().await).await
}

/// Collections a transaction writes to. `main` creates every collection at start-up (through
/// `ensure_indexes`); a test database is fresh, and a transaction that creates a collection
/// implicitly conflicts with a concurrent one doing the same ("namespace is already in use").
/// Only these are created, because every collection counts against Atlas's 500 per cluster.
const TRANSACTIONAL_COLLECTIONS: &[&str] = &[
    "purchase_receipts",
    "receipt_settlements",
    "supplier_credits",
    "cash_movements",
    "drawer_state",
    "register_sessions",
    "expenses",
    "expense_payments",
    "purchases",
    "stock_batches",
    "stock_movements",
    "product_serials",
];

/// Start-up work done once per database and process, however many tests start an app on it at
/// the same time: the transactional collections above, and the built-in cash categories (their
/// unique key index rejects a concurrent second seeding).
async fn start_up_once(db: &Database, db_handle: &simplebash_pos_backend::clients::db::Db) {
    static DONE: tokio::sync::Mutex<Option<std::collections::HashSet<String>>> =
        tokio::sync::Mutex::const_new(None);
    let mut done = DONE.lock().await;
    if done
        .get_or_insert_with(Default::default)
        .insert(db.name().to_string())
    {
        let existing = db.list_collection_names().await.expect("list collections");
        for name in TRANSACTIONAL_COLLECTIONS {
            if !existing.iter().any(|e| e == name) {
                db.create_collection(*name)
                    .await
                    .expect("create collection");
            }
        }
        simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories(
            db_handle,
        )
        .await
        .expect("failed to ensure the built-in cash categories");
    }
}

async fn spawn_mongo_app(db_name: String, document_server_url: String) -> TestApp {
    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.mongodb_db_name = db_name;

    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to test MongoDB");

    config.document_server_url = document_server_url;

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    start_up_once(&db, &db_handle).await;
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config: config.clone(),
        db: db_handle.clone(),
        document_server,
        reports_engine,
    };

    TestApp {
        router: app::build_router(state),
        db,
        db_handle,
        config,
    }
}

/// Builds the router against an isolated, clean SQLite test database.
#[allow(dead_code)]
pub async fn spawn_app_sqlite() -> TestApp {
    let mock_doc_server_url = start_mock_document_server().await;
    spawn_app_sqlite_with_document_server_url(mock_doc_server_url).await
}

/// Same as `spawn_app_sqlite`, but pointing at a custom mock document-server URL.
#[allow(dead_code)]
pub async fn spawn_app_sqlite_with_document_server_url(document_server_url: String) -> TestApp {
    dotenvy::dotenv().ok();

    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.database_type = simplebash_pos_backend::core::config::DatabaseType::Sqlite;
    let test_id = simplebash_pos_backend::core::id::generate_id("test");
    let test_db_path = format!("data/test_{test_id}.db");
    config.database_url = format!("sqlite://{test_db_path}?mode=rwc");

    let db_handle = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to test SQLite database");
    simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories(&db_handle)
        .await
        .expect("failed to ensure the built-in cash categories");

    config.document_server_url = document_server_url;
    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config: config.clone(),
        db: db_handle.clone(),
        document_server,
        reports_engine,
    };

    let mongo_client = match mongodb::Client::with_uri_str(&config.mongodb_uri).await {
        Ok(client) => client,
        Err(_) => mongodb::Client::with_uri_str("mongodb://localhost:27017")
            .await
            .expect("fallback mongo client"),
    };
    let db = mongo_client.database("simplebash_pos_test");

    TestApp {
        router: app::build_router(state),
        db,
        db_handle,
        config,
    }
}

/// Mints a JWT signed with the test app's own `jwt_secret`, carrying the
/// given role and permission set — shared across every test file that gates
/// a request behind `CurrentUser`/`AdminUser`/a specific permission without
/// exercising the real login flow (login itself is only hand-tested
/// end-to-end in `tests/auth_test.rs`).
#[allow(dead_code)]
pub fn mint_token(config: &Config, role: Option<Role>, permissions: &[&str]) -> String {
    let claims = Claims {
        sub: "test-user".to_string(),
        exp: 9_999_999_999,
        role,
        permissions: permissions.iter().map(|p| p.to_string()).collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("Test User".to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .unwrap()
}

/// Like `mint_token`, but for a multi-tenant deployment: the token carries the
/// tenant it was issued for (`tid`). `None` mints a token with no tenant.
#[allow(dead_code)]
pub fn mint_token_for_tenant(
    config: &Config,
    role: Option<Role>,
    permissions: &[&str],
    tid: Option<&str>,
) -> String {
    let claims = Claims {
        sub: "test-user".to_string(),
        exp: 9_999_999_999,
        role,
        permissions: permissions.iter().map(|p| p.to_string()).collect(),
        tid: tid.map(str::to_string),
        scope: None,
        did: None,
        name: Some("Test User".to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .unwrap()
}

/// A token for a registered sync device of `tid` (scope `device`, claim `did`).
#[allow(dead_code)]
pub fn mint_device_token(config: &Config, tid: &str, device_id: &str) -> String {
    let claims = Claims {
        sub: format!("device-{device_id}"),
        exp: 9_999_999_999,
        role: Some(Role::Admin),
        permissions: Vec::new(),
        tid: Some(tid.to_string()),
        scope: Some("device".to_string()),
        did: Some(device_id.to_string()),
        name: None,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .unwrap()
}

/// A device token for `device_id` of `tid`, registered with the cloud as an
/// app new enough to sync (`POST /sync/devices/register`), the way every desktop
/// agent starts. Push, pull and snapshot refuse a device that never registered.
#[allow(dead_code)]
pub async fn registered_device_token(app: &TestApp, tid: &str, device_id: &str) -> String {
    use tower::ServiceExt;
    let token = mint_device_token(&app.config, tid, device_id);
    let body = json!({ "deviceId": device_id, "deviceName": "Test PC", "appVersion": "1.23.0" });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sync/devices/register")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    // Not asserted: a revoked device is refused here too, and a test of that
    // checks the refusal of its own push/pull. A failed registration shows up
    // there as APP_UPDATE_REQUIRED.
    app.router.clone().oneshot(request).await.unwrap();
    token
}

/// Builds the real router in `TENANT_MODE=multi` against a uniquely named
/// throwaway Atlas database (`jtroute_<uuid24>`, well under Atlas's 38-byte
/// database-name cap). `TestApp.db` is the raw handle so the caller can inspect
/// every collection and drop the database when done.
#[allow(dead_code)]
pub async fn spawn_app_multi_tenant() -> TestApp {
    spawn_app_multi_tenant_with_secret(None).await
}

/// Same as `spawn_app_multi_tenant`, with `PROVISION_SECRET` set (or explicitly
/// unset), so the shop-provisioning endpoint can be exercised deterministically
/// whatever the environment holds.
#[allow(dead_code)]
pub async fn spawn_app_multi_tenant_with_secret(provision_secret: Option<&str>) -> TestApp {
    dotenvy::dotenv().ok();
    let mock_doc_server_url = start_mock_document_server().await;

    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.database_type = simplebash_pos_backend::core::config::DatabaseType::Mongo;
    config.tenant_mode = simplebash_pos_backend::core::config::TenantMode::Multi;
    config.provision_secret = provision_secret.map(str::to_string);
    config.mongodb_db_name = format!(
        "jtroute_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..24]
    );
    config.document_server_url = mock_doc_server_url;

    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to throwaway multi-tenant MongoDB");

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db_handle = simplebash_pos_backend::clients::db::Db::Mongo(
        simplebash_pos_backend::clients::tenant_db::TenantDatabase::multi_tenant(db.clone()),
    );
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config: config.clone(),
        db: db_handle.clone(),
        document_server,
        reports_engine,
    };

    TestApp {
        router: app::build_router(state),
        db,
        db_handle,
        config,
    }
}
