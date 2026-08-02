use std::sync::Arc;

use jana2u_pos_backend::{app, app::AppState, clients, core::config::Config};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().unwrap_or_else(|err| {
        tracing::error!(%err, "invalid configuration");
        std::process::exit(1);
    });
    let port = config.port;

    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to connect to MongoDB");
            std::process::exit(1);
        });
    tracing::info!(db = %config.mongodb_db_name, "connected to MongoDB");

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
    tracing::info!(port, "listening");

    axum::serve(listener, router).await.unwrap_or_else(|err| {
        tracing::error!(%err, "server error");
        std::process::exit(1);
    });
}
