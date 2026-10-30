use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::core::config::Config;
use crate::core::response::ApiResponse;
use crate::domain::HealthResponse;
use crate::modules;
use axum::Json;
use std::sync::Arc;

/// Shared application state injected into every handler via Axum's
/// `State` extractor. `Arc<Config>` because `Config` is read-only after
/// startup and cloned into every request's extensions; `Db` is
/// an internally-`Arc`'d handle (either Mongo or SQLite pool), so cloning it
/// per-request is cheap. `document_server` is behind its own `Arc` since
/// `DocumentServerClient` holds a `reqwest::Client` (already internally
/// `Arc`'d) plus a mutable template-key cache that must be shared, not
/// cloned, across requests.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub document_server: Arc<DocumentServerClient>,
    pub reports_engine: Arc<modules::reports::engine::AnalyticsEngine>,
}

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
                environment: Some(state.config.app_env.as_str().to_string()),
                version: Some(crate::core::constants::app_version::APP_VERSION.to_string()),
            },
            "Service is healthy",
        )),
    )
}

static SERVER_STARTED_AT: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

/// Seconds since the process started.
pub fn server_uptime_seconds() -> u64 {
    SERVER_STARTED_AT.elapsed().as_secs()
}
