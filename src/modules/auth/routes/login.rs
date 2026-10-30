use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::auth::{LoginRequest, LoginResponse};
use crate::modules::auth::service;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;

/// Best-effort client IP — this deployment sits behind a reverse proxy (see
/// `main.rs`'s "bound to 0.0.0.0, proxy sits outside this process" design),
/// so `X-Forwarded-For`/`X-Real-IP` are more meaningful here than the raw
/// TCP peer address would be. `None` when neither header is set (e.g. local
/// dev with no proxy in front) rather than a hard failure.
fn extract_ip_address(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

#[utoipa::path(post, path = "/login", tag = modules::AUTH, request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = ApiResponse<LoginResponse>),
        (status = 400, description = "Validation error (including a missing shopCode when TENANT_MODE=multi)", body = ErrorResponse),
        (status = 401, description = "Invalid username or password, or account deactivated (in multi-tenant mode also an unknown shopCode)", body = ErrorResponse),
    )
)]
pub(super) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    let ip_address = extract_ip_address(&headers);
    let user_agent = extract_user_agent(&headers);

    let response = service::login(&state.db, &state.config, body, ip_address, user_agent).await?;

    Ok(Json(ApiResponse::success(response, "Login successful")))
}
