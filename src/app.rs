use std::sync::Arc;

use axum::{Json, Router, http::Method};
use mongodb::Database;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    core::{config::Config, openapi::ApiDoc},
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
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = HealthResponse)
))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}
