use std::sync::Arc;

use axum::{Json, Router, http::Method};
use mongodb::Database;
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
    core::{config::Config, openapi::ApiDoc, response::ApiResponse},
    domain::HealthResponse,
    modules,
};

/// Shared application state injected into every handler via Axum's
/// `State` extractor. `Arc<Config>` because `Config` is read-only after
/// startup and cloned into every request's extensions; `Database` is
/// already an internally-`Arc`'d handle in the Mongo driver, so cloning it
/// per-request is cheap.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Database,
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
        );

    let (router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .nest("/api", api_router)
        .split_for_parts();

    router
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(cors)
        .layer(trace)
        .with_state(state)
}

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = ApiResponse<HealthResponse>)
))]
/// Liveness check — always returns 200 if the process is up and able to
/// handle a request at all. Doesn't touch Mongo, so it can't distinguish
/// "server up, database down"; use a module's own status route or a real
/// query for that.
async fn health() -> Json<ApiResponse<HealthResponse>> {
    Json(ApiResponse::success(
        HealthResponse {
            status: "ok".to_string(),
        },
        "Service is healthy",
    ))
}
