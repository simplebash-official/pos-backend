// Thin binary: load config, connect to Mongo, build the router, serve.
// Each step below fails fast (log + `process::exit(1)`) rather than
// letting the server start in a half-working state — a bad `.env`,
// unreachable Mongo, or an already-bound port should never look like a
// running server that then fails on the first real request.

use std::sync::Arc;

use jana2u_pos_backend::{app, app::AppState, clients, core::config::Config, core::logging};

#[tokio::main]
async fn main() {
    // `.ok()`: a missing `.env` is fine in prod (real env vars are already
    // set); `Config::from_env()` below is what actually enforces the
    // required variables are present..
    dotenvy::dotenv().ok();
    logging::init("jana2u_pos_backend=info,tower_http=info,info");
    // Desktop only: lets the shell change logging while we run.
    logging::control::spawn_stdin_control();

    let config = Config::from_env().unwrap_or_else(|err| {
        tracing::error!(%err, "invalid configuration");
        std::process::exit(1);
    });
    let port = config.port;
    let bind_addr = config.bind_addr.clone();
    tracing::info!(
        category = "lifecycle",
        event = "startup.config",
        version = env!("CARGO_PKG_VERSION"),
        database_type = ?config.database_type,
        bind_addr = %config.bind_addr,
        port = config.port,
        auto_seed = config.auto_seed,
        document_server_url = %config.document_server_url,
        generated_documents_dir = %config.generated_documents_dir,
        log_settings = ?logging::settings(),
        "backend configuration loaded"
    );

    tracing::info!(
        database_type = ?config.database_type,
        "Connecting to database..."
    );
    let db = clients::db::connect_from_config(&config)
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to connect to database");
            std::process::exit(1);
        });
    tracing::info!(database_type = ?config.database_type, "Successfully connected to database");

    if let Some(mongo_db) = db.as_mongo() {
        // Best-effort, and deliberately not fatal — see `ensure_indexes`.
        clients::indexes::ensure_indexes(mongo_db).await;
    }

    if config.auto_seed {
        tracing::info!("AUTO_SEED is enabled — checking and seeding database...");
        match jana2u_pos_backend::seeds::seed_all(&db).await {
            Ok(summary) => {
                tracing::info!(
                    admin = %summary.admin.message,
                    providers = summary.providers.categories_created,
                    suppliers = summary.suppliers.suppliers_created,
                    customers = summary.customers.customers_created,
                    products = summary.inventory.products_created,
                    "Automated database seeding finished successfully"
                );
            }
            Err(err) => {
                tracing::error!(%err, "Automated database seeding encountered an error");
            }
        }
    }

    let document_server = clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    );

    tracing::info!(
        "Checking document-server at {}...",
        config.document_server_url
    );
    // Poll generously: a cold document-server does a blocking Typst engine
    // warm-up (font parsing + trial compiles) before it answers /health,
    // which can take a few seconds on first launch of the desktop bundle.
    // A failure here is logged, not fatal — the only thing that needs
    // document-server is invoice/receipt printing, and the process
    // supervisor (compose healthcheck / Tauri sidecar gate) already
    // orders startup.
    match document_server.wait_until_ready(40).await {
        Ok(()) => tracing::info!("Successfully connected to document-server"),
        Err(err) => tracing::warn!(
            %err,
            "document-server not reachable at startup — document rendering will fail until it is up"
        ),
    }

    let reports_engine =
        Arc::new(jana2u_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()));
    reports_engine.init().await;

    let state = AppState {
        config: Arc::new(config),
        db,
        document_server: Arc::new(document_server),
        reports_engine,
    };
    let router = app::build_router(state);

    // Defaults to 0.0.0.0 so a container/reverse proxy can route external
    // traffic in; `BIND_ADDR=127.0.0.1` restricts it to loopback (used by
    // the Tauri desktop bundle, where the frontend runs in the same host).
    let listener = tokio::net::TcpListener::bind((bind_addr.as_str(), port))
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, %bind_addr, "failed to bind listener");
            std::process::exit(1);
        });

    tracing::info!("Server started successfully on port {}", port);
    tracing::info!("   - API Base URL:  http://localhost:{}/api", port);
    tracing::info!("   - Swagger Docs:  http://localhost:{}/docs", port);
    tracing::info!(
        "   - OpenAPI Spec:  http://localhost:{}/api-docs/openapi.json",
        port
    );

    axum::serve(listener, router).await.unwrap_or_else(|err| {
        tracing::error!(%err, "server error");
        std::process::exit(1);
    });
}
