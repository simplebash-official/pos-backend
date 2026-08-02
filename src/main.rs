use std::sync::Arc;

use jana2u_pos_backend::{app, app::AppState, clients, core::config::Config};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("jana2u_pos_backend=info,tower_http=info,info")),
        )
        .init();

    let config = Config::from_env().unwrap_or_else(|err| {
        tracing::error!(%err, "invalid configuration");
        std::process::exit(1);
    });
    let port = config.port;

    tracing::info!("Connecting to MongoDB database '{}'...", config.mongodb_db_name);
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to connect to MongoDB");
            std::process::exit(1);
        });
    tracing::info!(db = %config.mongodb_db_name, "Successfully connected to MongoDB");

    let state = AppState {
        config: Arc::new(config),
        db,
    };
    let router = app::build_router(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to bind listener");
            std::process::exit(1);
        });

    tracing::info!("Server started successfully on port {}", port);
    tracing::info!("   - API Base URL:  http://localhost:{}/api", port);
    tracing::info!("   - Swagger Docs:  http://localhost:{}/docs", port);
    tracing::info!("   - OpenAPI Spec:  http://localhost:{}/api-docs/openapi.json", port);

    axum::serve(listener, router).await.unwrap_or_else(|err| {
        tracing::error!(%err, "server error");
        std::process::exit(1);
    });
}

