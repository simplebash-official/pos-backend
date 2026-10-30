use super::app_state::AppState;
use super::server_uptime_seconds::server_uptime_seconds;

use crate::core::response::ApiResponse;
use crate::domain::HealthResponse;

use axum::Json;

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = ApiResponse<HealthResponse>)
))]
/// Liveness check — returns 200 with service health status, live uptime, environment, and version.
pub(super) async fn health(
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
