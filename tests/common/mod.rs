use std::sync::Arc;

use axum::Router;
use jana2u_pos_backend::{app, app::AppState, clients, core::config::Config};

pub struct TestApp {
    pub router: Router,
}

/// Builds the real router against a Mongo test database. Reads connection
/// details from the environment (`.env` is loaded, same as production) but
/// always targets `MONGODB_TEST_DB_NAME` so tests never touch dev data.
pub async fn spawn_app() -> TestApp {
    dotenvy::dotenv().ok();

    let mut config = Config::from_env().expect("invalid configuration for test run");
    config.mongodb_db_name =
        std::env::var("MONGODB_TEST_DB_NAME").unwrap_or_else(|_| "jana2u_pos_test".to_string());

    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to test MongoDB");

    let state = AppState {
        config: Arc::new(config),
        db,
    };

    TestApp {
        router: app::build_router(state),
    }
}
