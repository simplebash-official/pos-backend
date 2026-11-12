//! Thin binary: load config, connect to Mongo, build the router, serve.
//! Each step below fails fast (log + `process::exit(1)`) rather than
//! letting the server start in a half-working state — a bad `.env`,
//! unreachable Mongo, or an already-bound port should never look like a
//! running server that then fails on the first real request.

use std::sync::Arc;

use simplebash_pos_backend::{app, clients};

use simplebash_pos_backend::app::AppState;

use simplebash_pos_backend::core::config::Config;

use simplebash_pos_backend::core::logging;

#[tokio::main]
async fn main() {
    // `.ok()`: a missing `.env` is fine in prod (real env vars are already set); the
    // configuration below is what enforces that every required variable is present.
    dotenvy::dotenv().ok();
    // Logging is configured by the configuration itself, so a bad one can only be reported
    // on stderr.
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("invalid configuration: {err}");
        std::process::exit(1);
    });
    logging::init(&config.logging);
    // Desktop only: lets the shell change logging while we run.
    logging::control::spawn_stdin_control();

    let port = config.port;
    let bind_addr = config.bind_addr.clone();
    tracing::info!(
        category = "lifecycle",
        event = "startup.config",
        version = simplebash_pos_backend::core::constants::app_version::APP_VERSION,
        database = ?config.database,
        bind_addr = %config.bind_addr,
        port = config.port,
        auto_seed = config.auto_seed,
        document_server_url = %config.document_server_url,
        generated_documents_dir = %config.generated_documents_dir,
        log_settings = ?logging::settings(),
        "backend configuration loaded"
    );

    tracing::info!(database = ?config.database, "Connecting to database...");
    let db = clients::connect_from_config(&config)
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to connect to database");
            std::process::exit(1);
        });
    tracing::info!(database = ?config.database, "Successfully connected to database");

    if let Some(mongo_db) = db.as_mongo() {
        // Best-effort, and deliberately not fatal — see `ensure_indexes`.
        clients::indexes::ensure_indexes(mongo_db.unscoped(), config.tenant_mode).await;
    }

    // The drawer's starting categories, drawer rules, and shop profile are required reference data,
    // not demo data, so they are ensured on every start (idempotent).
    if config.tenant_mode == simplebash_pos_backend::core::config::TenantMode::Multi {
        if config.auto_seed {
            tracing::warn!(
                "AUTO_SEED=true is ignored in multi-tenant mode; multi-tenant shops are initialized per tenant"
            );
        }
        match simplebash_pos_backend::modules::tenants::service::ensure_existing_tenants_data(&db)
            .await
        {
            Ok(tenants) => {
                tracing::info!(
                    tenants,
                    "Verified required system data for existing tenants"
                );
            }
            Err(err) => {
                tracing::error!(%err, "failed to ensure required system data for existing tenants");
                std::process::exit(1);
            }
        }
    } else {
        if let Err(err) =
            simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories(&db)
                .await
        {
            tracing::error!(%err, "failed to ensure the built-in cash categories");
            std::process::exit(1);
        }

        if config.auto_seed {
            tracing::info!("AUTO_SEED is enabled — checking and seeding database...");
            let summary = simplebash_pos_backend::seeds::seed_all(&db, &config.seed_admin)
                .await
                .unwrap_or_else(|err| {
                    tracing::error!(%err, "automated database seeding failed");
                    std::process::exit(1);
                });
            tracing::info!(
                admin = %summary.admin.message,
                providers = summary.providers.categories_created,
                suppliers = summary.suppliers.suppliers_created,
                customers = summary.customers.customers_created,
                products = summary.inventory.products_created,
                "Automated database seeding finished successfully"
            );
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

    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()),
    );
    reports_engine.init().await;

    // Multi-tenant cloud only: the single change-stream consumer that feeds the
    // per-tenant sync change log (it holds a lease, so extra instances stand
    // by), and this instance's watcher announcing new log rows to the devices
    // and browsers subscribed to `GET /api/sync/events`.
    if config.tenant_mode == simplebash_pos_backend::core::config::TenantMode::Multi {
        tokio::spawn(
            simplebash_pos_backend::modules::sync::cloud_capture::run_consumer(db.clone()),
        );
        tokio::spawn(simplebash_pos_backend::modules::sync::live::run_watcher(
            db.clone(),
        ));
    }

    let state = AppState {
        config: Arc::new(config),
        db,
        document_server: Arc::new(document_server),
        reports_engine,
    };
    let router = app::build_router(state);

    // `BIND_ADDR=0.0.0.0` lets a container/reverse proxy route external traffic in;
    // `127.0.0.1` restricts it to loopback (the Tauri desktop bundle).
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
