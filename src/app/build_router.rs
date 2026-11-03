use super::app_items as health;
use super::app_items::AppState;
use super::app_items::server_uptime_seconds;

use crate::core::middleware::timing::add_processing_time_to_body;
use crate::core::openapi::ApiDoc;

use crate::modules;
use axum::http::Method;
use axum::{Router, middleware};

use tower_http::LatencyUnit;
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

/// Assembles the full HTTP router: the top-level `/health` check, every
/// feature module nested under `/api/<module>` (see `mod_names` below),
/// Swagger UI, CORS, and request tracing — this is the one place all of
/// that gets wired together, called once from `main.rs`.
pub fn build_router(state: AppState) -> Router {
    // The request logger reads these settings; the first call in a process wins.
    crate::core::logging::install(&state.config.logging);
    // Browsers are only answered from the configured origin list — never `*`: a desktop backend
    // listens on 127.0.0.1, so a wildcard would let any web page the shop owner visits drive
    // this API. `Config` has already checked that every origin is a valid header value.
    let allow_origin = tower_http::cors::AllowOrigin::list(
        state.config.cors_allowed_origins.iter().map(|origin| {
            origin
                .parse::<axum::http::HeaderValue>()
                .expect("CORS_ALLOWED_ORIGINS was validated when the configuration loaded")
        }),
    );
    let cors = CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(tower_http::cors::Any)
        // Live calculations send a JSON POST per keystroke; without this Chromium repeats the
        // preflight every 5 seconds, which doubles the round trips of a figure that is waiting.
        .max_age(std::time::Duration::from_secs(86_400))
        // A recorded label print answers with its PDF, so the job's key rides in a
        // header the browser must be allowed to read across origins (desktop app).
        .expose_headers([axum::http::HeaderName::from_static(
            crate::modules::labels::routes::LABEL_JOB_KEY_HEADER,
        )]);

    // Logs every request/response to the terminal (method, path, status,
    // latency) at INFO level, so nothing extra needs to be set (e.g.
    // RUST_LOG) to see request activity — tower_http's defaults for these
    // callbacks are DEBUG, which stays silent under the app's default filter.
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        );

    use crate::core::constants::modules as mod_names;

    // There is deliberately no global authentication layer below —
    // authentication is opt-in *per handler*, enforced by declaring
    // `CurrentUser` or `AdminUser` as a handler argument (see
    // `core::middleware::auth`). The consequence is the thing to remember
    // when adding a route: **a handler that declares neither is public**,
    // and nothing in the type system or the OpenAPI derive will flag it.
    // Inventory's product/stock endpoints were unauthenticated for exactly
    // this reason until they were given `CurrentUser` +
    // `require_permission`.
    //
    // The complete set of intentionally-public routes is:
    //   GET  /api/health              liveness probe, static payload
    //   POST /api/auth/login          issues the token
    //   GET  /api/auth/shop/{code}    public shop lookup for multi-tenant branding
    //   GET  /api/system/setup-status query initial installation and setup status
    //   POST /api/system/setup        bootstrap administrator and initialize database
    //   POST /api/internal/provision  identity -> POS shop hand-off; authenticated by the
    //                                 shared X-Provision-Secret header, blocked at nginx
    //   PUT  /api/internal/tenants/{tenant_id}/status, GET .../usage, POST /api/internal/tenants/usage,
    //   GET  .../document-selections, POST .../document-selections/reset,
    //   GET  .../users, GET .../logins, GET .../activities
    //                                 operator console (suspend a shop, read usage, designs in use, users, logins, activities); same secret
    //   GET  /docs, /api-docs/openapi.json   Swagger UI
    //   GET  /api/{inventory,billing,reports}
    //                                 static module-status stubs
    //
    // `repairs`/`print-jobs` were on this list too until they gained real
    // handlers (Phase 3 of the billing-backend migration) — their `GET /`
    // is now the real "list" endpoint, gated by `CurrentUser` like every
    // other read in those modules, not a stub. Left here as the concrete
    // example of the warning two lines below.
    //
    // Anything not on that list must take an auth extractor, and
    // `tests/authorization_test.rs` fails the build if one doesn't — it walks
    // every operation in the generated OpenAPI document and asserts an
    // unauthenticated request gets 401 unless the route is on the matching
    // allowlist there. Keep the two lists in step. The stubs are the pattern
    // to be careful with: when one of those modules gains real handlers, they
    // need their own extractor — they do not inherit one.
    let api_router: OpenApiRouter<AppState> = OpenApiRouter::new()
        .routes(routes!(health::health))
        .nest(
            &format!("/{}", mod_names::AUTH),
            modules::auth::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::BILLING),
            modules::billing::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::CALC),
            modules::calc::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::CUSTOMERS),
            modules::customers::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::EMPLOYEES),
            modules::employees::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::INVENTORY),
            modules::inventory::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::LABELS),
            modules::labels::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::WARRANTIES),
            modules::warranties::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::WARRANTY_CLAIMS.replace('_', "-")),
            modules::warranties::routes::claims_router(),
        )
        .nest(
            &format!("/{}", mod_names::REPAIR_SERVICES.replace('_', "-")),
            modules::repairs::routes::services_router(),
        )
        // `PRINT_JOBS` is `"print_jobs"` (matches the Rust module name and
        // OpenAPI tag) but the URL should read `/print-jobs`, so the
        // underscore is swapped for a hyphen only at mount time — the
        // constant itself stays snake_case everywhere else it's used.
        .nest(
            &format!("/{}", mod_names::PRINT_JOBS.replace('_', "-")),
            modules::print_jobs::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::QUOTATIONS),
            modules::quotations::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::REPAIRS),
            modules::repairs::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::REPORTS),
            modules::reports::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::SUPPLIERS),
            modules::suppliers::routes::router(),
        )
        // `SUPPLIER_PRODUCTS` is `"supplier_products"` internally but the URL
        // should read `/supplier-products` — same underscore-to-hyphen swap
        // as `PRINT_JOBS` above, only at mount time.
        .nest(
            &format!("/{}", mod_names::SUPPLIER_PRODUCTS.replace('_', "-")),
            modules::supplier_products::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::PURCHASES),
            modules::purchases::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::PURCHASE_RECEIPTS.replace('_', "-")),
            modules::purchases::routes::receipts_router(),
        )
        .nest(
            &format!("/{}", mod_names::SUPPLIER_CREDITS.replace('_', "-")),
            modules::purchases::routes::credits_router(),
        )
        .nest(
            &format!("/{}", mod_names::SEQUENCES),
            modules::sequences::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::SYNC),
            modules::sync::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::USERS),
            modules::users::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::IMPORTS),
            modules::imports::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::BACKUP),
            modules::backup::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::SYSTEM),
            modules::system::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::SETTINGS),
            modules::settings::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::SESSIONS),
            modules::sessions::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::CASH_CATEGORIES.replace('_', "-")),
            modules::cash_categories::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::EXPENSES),
            modules::expenses::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::ATTACHMENTS),
            modules::attachments::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::CASH_SETTINGS.replace('_', "-")),
            modules::cash_settings::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::BALANCES),
            modules::balances::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::ACCOUNTS),
            modules::accounts::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::FINANCE_REPORTS.replace('_', "-")),
            modules::finance_reports::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::INCOMES),
            modules::incomes::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::RECURRING_EXPENSES.replace('_', "-")),
            modules::recurring_expenses::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::PAYEES),
            modules::payees::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::DOCUMENT_PACKAGES),
            modules::document_packages::routes::router(),
        )
        .nest("/internal", modules::tenants::routes::router());

    let (router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .nest("/api", api_router)
        .split_for_parts();

    router
        .route("/", axum::routing::get(portal))
        .route("/api", axum::routing::get(portal))
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        // Inside CORS/trace so a 429 still carries CORS headers and is logged.
        // Inside CORS/trace so a 429 still carries CORS headers and is logged.
        .layer(axum::middleware::from_fn_with_state(
            std::sync::Arc::new(crate::core::rate_limit::RequestLimits::new(
                &state.config.rate_limits,
            )),
            crate::core::rate_limit::limit_requests,
        ))
        .layer(cors)
        .layer(trace)
        // Desktop: wakes the sync agent after each write so it uploads at
        // once (a cheap no-op where nobody listens).
        .layer(middleware::from_fn(
            crate::modules::sync::live::local_write_tick,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::core::middleware::idempotency::handle_idempotency,
        ))
        .layer(middleware::from_fn(
            crate::core::middleware::sync_headers::add_server_time_header,
        ))
        .layer(middleware::from_fn(add_processing_time_to_body))
        // Puts the caller's tenant in scope (no-op unless TENANT_MODE=multi).
        // Outside idempotency so its key store is tenant-scoped as well.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::core::middleware::tenant::tenant_context,
        ))
        // Outermost: every layer below and every handler log line runs
        // inside this request's span (request_id / user_id).
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::core::logging::request::log_requests,
        ))
        .with_state(state)
}

const PORTAL_HTML: &str = include_str!("../portal.html");

async fn portal(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> (
    [(axum::http::HeaderName, &'static str); 1],
    axum::response::Html<String>,
) {
    let uptime = server_uptime_seconds();
    let version = crate::core::constants::app_version::APP_VERSION;
    let (env_class, display_env) = match state.config.app_env {
        crate::core::config::AppEnvironment::Production => ("env-prod", "Production"),
        crate::core::config::AppEnvironment::Development => ("env-dev", "Development"),
    };

    let rendered = PORTAL_HTML
        .replace("__VERSION__", version)
        .replace("__ENVIRONMENT__", display_env)
        .replace("__ENV_CLASS__", env_class)
        .replace("__INITIAL_UPTIME__", &uptime.to_string());

    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::response::Html(rendered),
    )
}
