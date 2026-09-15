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

/// Assembles the full HTTP router: the top-level `/health` check, every
/// feature module nested under `/api/<module>` (see `mod_names` below),
/// Swagger UI, CORS, and request tracing — this is the one place all of
/// that gets wired together, called once from `main.rs`.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
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
    //   GET  /api/system/setup-status query initial installation and setup status
    //   POST /api/system/setup        bootstrap administrator and initialize database
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
        );

    let (router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .nest("/api", api_router)
        .split_for_parts();

    router
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
        // Outermost: every layer below and every handler log line runs
        // inside this request's span (request_id / user_id).
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::core::logging::request::log_requests,
        ))
        .with_state(state)
}

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = ApiResponse<HealthResponse>)
))]
/// Liveness check — always returns 200 if the process is up and able to
/// handle a request at all. Doesn't touch Mongo, so it can't distinguish
/// "server up, database down"; use a module's own status route or a real
/// query for that. Returns Cache-Control: no-store.
async fn health() -> (
    [(axum::http::HeaderName, &'static str); 1],
    Json<ApiResponse<HealthResponse>>,
) {
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(ApiResponse::success(
            HealthResponse {
                status: "ok".to_string(),
            },
            "Service is healthy",
        )),
    )
}
