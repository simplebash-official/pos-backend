//! Test-harness pieces: spawn app.

use std::sync::Arc;

use super::mock_document_server::start_mock_document_server;
use super::test_app::TestApp;
use super::test_config::{mongo_overrides, mongo_uri, test_config};
use mongodb::Database;
use simplebash_pos_backend::app::AppState;
use simplebash_pos_backend::{app, clients};

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
    let db_name = std::env::var("MONGODB_TEST_DB_NAME").expect(
        "MONGODB_TEST_DB_NAME must name the throwaway test database (use `make check-atlas`)",
    );
    assert!(
        db_name.starts_with("jt"),
        "MONGODB_TEST_DB_NAME must be a throwaway `jt*` database, not {db_name}"
    );
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
pub(super) const TRANSACTIONAL_COLLECTIONS: &[&str] = &[
    "purchase_receipts",
    "receipt_settlements",
    "supplier_credits",
    "receipt_charges",
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
pub(super) async fn start_up_once(
    db: &Database,
    db_handle: &simplebash_pos_backend::clients::db::Db,
) {
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

pub(super) async fn spawn_mongo_app(db_name: String, document_server_url: String) -> TestApp {
    let uri = mongo_uri();
    let mut overrides = mongo_overrides(&uri, &db_name).to_vec();
    overrides.push(("DOCUMENT_SERVER_URL", document_server_url.as_str()));
    let config = test_config(&overrides);

    let db = clients::mongo::connect(&uri, &db_name)
        .await
        .expect("failed to connect to test MongoDB");

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
        db: Some(db),
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

    let test_id = simplebash_pos_backend::core::id::generate_id("test");
    let test_db_path = format!("data/test_{test_id}.db");
    let database_url = format!("sqlite://{test_db_path}?mode=rwc");
    let config = test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("DOCUMENT_SERVER_URL", document_server_url.as_str()),
    ]);

    let db_handle = clients::connect_from_config(&config)
        .await
        .expect("failed to connect to test SQLite database");
    simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories(&db_handle)
        .await
        .expect("failed to ensure the built-in cash categories");

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

    TestApp {
        router: app::build_router(state),
        db: None,
        db_handle,
        config,
    }
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

    let uri = mongo_uri();
    let db_name = format!(
        "jtroute_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..24]
    );
    let mut overrides = mongo_overrides(&uri, &db_name).to_vec();
    overrides.push(("TENANT_MODE", "multi"));
    overrides.push(("DOCUMENT_SERVER_URL", mock_doc_server_url.as_str()));
    if let Some(secret) = provision_secret {
        overrides.push(("PROVISIONING", "enabled"));
        overrides.push(("PROVISION_SECRET", secret));
    }
    let config = test_config(&overrides);

    let db = clients::mongo::connect(&uri, &db_name)
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
        db: Some(db),
        db_handle,
        config,
    }
}
