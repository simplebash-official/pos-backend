use std::sync::Arc;

use axum::Router;
use jana2u_pos_backend::{
    app, app::AppState, clients, core::config::Config, core::middleware::auth::Claims,
    domain::users::Role,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::Database;
use serde_json::json;

pub struct TestApp {
    pub router: Router,
    // Not every integration test binary that includes this module needs
    // direct DB access (e.g. to seed data outside the API) or the config
    // (e.g. to mint a JWT with `jwt_secret`) — these fields are dead code
    // from the perspective of whichever one doesn't.
    #[allow(dead_code)]
    pub db: Database,
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

fn mock_templates_json() -> serde_json::Value {
    json!({
        "success": true,
        "message": "Templates fetched",
        "data": {
            "templates": [
                {
                    "key": "tpl_a4_invoice",
                    "name": "doc_temp_vEf0Y7jQHQj2rIuO",
                    "description": "A4 Invoice",
                    "dataSchema": {
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "type": "object",
                        "additionalProperties": true,
                        "required": ["invoiceNumber"],
                        "properties": {
                            "invoiceNumber": { "type": "string" },
                            "logoUrl": { "type": "string" }
                        }
                    }
                },
                {
                    "key": "tpl_thermal_receipt",
                    "name": "doc_temp_4pz79z5iba7TcEIp",
                    "description": "Thermal Receipt",
                    "dataSchema": {
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "type": "object",
                        "additionalProperties": true,
                        "required": ["paperWidthMm", "invoiceNumber"],
                        "properties": {
                            "paperWidthMm": { "type": "integer", "enum": [58, 80] },
                            "invoiceNumber": { "type": "string" }
                        }
                    }
                },
                {
                    "key": "tpl_credit_note",
                    "name": "doc_temp_5DHl8hUQTX3oLBSR",
                    "description": "Credit Note",
                    "dataSchema": {
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "type": "object",
                        "additionalProperties": true,
                        "required": ["creditNoteNumber"],
                        "properties": {
                            "creditNoteNumber": { "type": "string" }
                        }
                    }
                },
                {
                    "key": "tpl_analytics_report",
                    "name": "doc_temp_An1yT1csRep0rtV1",
                    "description": "Analytics Report",
                    "dataSchema": {
                        "$schema": "http://json-schema.org/draft-07/schema#",
                        "type": "object",
                        "additionalProperties": true,
                        "required": ["generatedAt", "periodLabel", "granularityLabel", "kpis", "timeseries"],
                        "properties": {
                            "generatedAt": { "type": "string" },
                            "periodLabel": { "type": "string" },
                            "granularityLabel": { "type": "string" },
                            "kpis": { "type": "array" },
                            "timeseries": { "type": "object" }
                        }
                    }
                },
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
            get(move || {
                let templates_response = templates_response.clone();
                async move { Json(templates_response) }
            }),
        )
        .route(
            "/api/render/{template_key}",
            post(
                |Path(template_key): Path<String>,
                 headers: HeaderMap,
                 _body: axum::body::Bytes| async move {
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
                            format!("%PDF-1.7 mock rendered PDF for template {template_key}")
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

    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.mongodb_db_name =
        std::env::var("MONGODB_TEST_DB_NAME").unwrap_or_else(|_| "jana2u_pos_test".to_string());

    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to test MongoDB");

    config.document_server_url = document_server_url;

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        document_server,
    };

    TestApp {
        router: app::build_router(state),
        db,
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
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .unwrap()
}
