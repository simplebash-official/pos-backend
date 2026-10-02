use std::sync::Arc;

use axum::{Json, Router, http::Method, middleware};
use tower_http::{
    LatencyUnit,
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    clients::{db::Db, document_server::DocumentServerClient},
    core::{
        config::Config, middleware::timing::add_processing_time_to_body, openapi::ApiDoc,
        response::ApiResponse,
    },
    domain::HealthResponse,
    modules,
};

/// Shared application state injected into every handler via Axum's
/// `State` extractor. `Arc<Config>` because `Config` is read-only after
/// startup and cloned into every request's extensions; `Db` is
/// an internally-`Arc`'d handle (either Mongo or SQLite pool), so cloning it
/// per-request is cheap. `document_server` is behind its own `Arc` since
/// `DocumentServerClient` holds a `reqwest::Client` (already internally
/// `Arc`'d) plus a mutable template-key cache that must be shared, not
/// cloned, across requests.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub document_server: Arc<DocumentServerClient>,
    pub reports_engine: Arc<modules::reports::engine::AnalyticsEngine>,
}

/// Browser origins a single-shop backend answers when `CORS_ALLOWED_ORIGINS`
/// is unset: the Tauri desktop webview (macOS/Linux `tauri://localhost`,
/// Windows `http(s)://tauri.localhost`) and the Vite dev server. A web
/// deployment whose UI is served from another origin must list it in
/// `CORS_ALLOWED_ORIGINS`.
pub const DEFAULT_SINGLE_SHOP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:5173",
    "http://127.0.0.1:5173",
];

/// Assembles the full HTTP router: the top-level `/health` check, every
/// feature module nested under `/api/<module>` (see `mod_names` below),
/// Swagger UI, CORS, and request tracing — this is the one place all of
/// that gets wired together, called once from `main.rs`.
pub fn build_router(state: AppState) -> Router {
    // Browsers are only answered from an explicit origin list — never `*`:
    // a desktop backend listens on 127.0.0.1, so a wildcard would let any web
    // page the shop owner visits drive this API. Multi-tenant deployments with
    // no `CORS_ALLOWED_ORIGINS` allow no browser origin at all (fail closed);
    // single-shop ones fall back to the desktop webview + local dev origins.
    let configured: Vec<&str> = state
        .config
        .cors_allowed_origins
        .iter()
        .map(String::as_str)
        .collect();
    let origin_list: Vec<&str> = if !configured.is_empty() {
        configured
    } else if state.config.tenant_mode == crate::core::config::TenantMode::Multi {
        tracing::warn!(
            "TENANT_MODE=multi without CORS_ALLOWED_ORIGINS: no browser origin is allowed"
        );
        Vec::new()
    } else {
        DEFAULT_SINGLE_SHOP_ORIGINS.to_vec()
    };
    let allow_origin = tower_http::cors::AllowOrigin::list(
        origin_list
            .iter()
            .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok()),
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
        .allow_headers(tower_http::cors::Any);

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
        .routes(routes!(health))
        .nest(
            &format!("/{}", mod_names::AUTH),
            modules::auth::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::BILLING),
            modules::billing::routes::router(),
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
        // `PRINT_JOBS` is `"print_jobs"` (matches the Rust module name and
        // OpenAPI tag) but the URL should read `/print-jobs`, so the
        // underscore is swapped for a hyphen only at mount time — the
        // constant itself stays snake_case everywhere else it's used.
        .nest(
            &format!("/{}", mod_names::PRINT_JOBS.replace('_', "-")),
            modules::print_jobs::routes::router(),
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
        .nest("/internal", modules::tenants::routes::router());

    let (router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .nest("/api", api_router)
        .split_for_parts();

    router
        .route("/", axum::routing::get(portal))
        .route("/api", axum::routing::get(portal))
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(cors)
        .layer(trace)
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

const PORTAL_HTML: &str = include_str!("portal.html");

static SERVER_STARTED_AT: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

pub fn server_uptime_seconds() -> u64 {
    SERVER_STARTED_AT.elapsed().as_secs()
}

async fn portal(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> (
    [(axum::http::HeaderName, &'static str); 1],
    axum::response::Html<String>,
) {
    let uptime = server_uptime_seconds();
    let version = env!("CARGO_PKG_VERSION");
    let env_name = &state.config.app_env;
    let env_class =
        if env_name.eq_ignore_ascii_case("production") || env_name.eq_ignore_ascii_case("prod") {
            "env-prod"
        } else {
            "env-dev"
        };

    let display_env = match env_name.to_lowercase().as_str() {
        "production" | "prod" => "Production".to_string(),
        "development" | "dev" => "Development".to_string(),
        "staging" | "stage" => "Staging".to_string(),
        "test" | "testing" => "Test".to_string(),
        s => {
            let mut chars = s.chars();
            match chars.next() {
                None => "Development".to_string(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    };

    let rendered = PORTAL_HTML
        .replace("__VERSION__", version)
        .replace("__ENVIRONMENT__", &display_env)
        .replace("__ENV_CLASS__", env_class)
        .replace("__INITIAL_UPTIME__", &uptime.to_string());

    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::response::Html(rendered),
    )
}

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = ApiResponse<HealthResponse>)
))]
/// Liveness check — returns 200 with service health status, live uptime, environment, and version.
async fn health(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> (
    [(axum::http::HeaderName, &'static str); 1],
    Json<ApiResponse<HealthResponse>>,
) {
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(ApiResponse::success(
            HealthResponse {
                status: "ok".to_string(),
                uptime_seconds: Some(server_uptime_seconds()),
                environment: Some(state.config.app_env.clone()),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            },
            "Service is healthy",
        )),
    )
}
