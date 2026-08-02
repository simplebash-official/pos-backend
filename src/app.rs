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

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Database,
}

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

    let api_router: OpenApiRouter<AppState> = OpenApiRouter::new()
        .routes(routes!(health))
        .nest("/auth", modules::auth::routes::router())
        .nest("/billing", modules::billing::routes::router())
        .nest("/customers", modules::customers::routes::router())
        .nest("/inventory", modules::inventory::routes::router())
        .nest("/print-jobs", modules::print_jobs::routes::router())
        .nest("/repairs", modules::repairs::routes::router())
        .nest("/reports", modules::reports::routes::router());

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
async fn health() -> Json<ApiResponse<HealthResponse>> {
    Json(ApiResponse::success(
        HealthResponse {
            status: "ok".to_string(),
        },
        "Service is healthy",
    ))
}
