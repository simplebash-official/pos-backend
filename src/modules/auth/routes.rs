// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). `login` is the one public (unauthenticated) route in
// this file; `me`/`sessions` require `CurrentUser`, and `sessions`
// additionally checks `require_permission(SESSIONS_VIEW)` as the first line
// of its body (see `core::middleware::auth::CurrentUser` for why that's a
// method call rather than a dedicated extractor type). The placeholder
// `GET /` status route this module used to have is gone — these are its
// real endpoints now, matching the precedent that `suppliers`/
// `supplier_products`/`purchases` don't carry one once real routes exist.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::{
        auth::{
            LoginRequest, LoginResponse, LoginSessionListQuery, LoginSessionsResponse,
            ShopLookupResponse,
        },
        users::User,
    },
    modules::auth::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(lookup_shop))
        .routes(routes!(me))
        .routes(routes!(list_sessions))
}

// ============================================================================
// Helpers
// ============================================================================

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

// ============================================================================
// Login / current user
// ============================================================================

#[utoipa::path(post, path = "/login", tag = modules::AUTH, request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = ApiResponse<LoginResponse>),
        (status = 400, description = "Validation error (including a missing shopCode when TENANT_MODE=multi)", body = ErrorResponse),
        (status = 401, description = "Invalid email or password, or account deactivated (in multi-tenant mode also an unknown shopCode)", body = ErrorResponse),
    )
)]
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    let ip_address = extract_ip_address(&headers);
    let user_agent = extract_user_agent(&headers);

    let response = service::login(&state.db, &state.config, body, ip_address, user_agent).await?;

    Ok(Json(ApiResponse::success(response, "Login successful")))
}

#[utoipa::path(get, path = "/shop/{code}", tag = modules::AUTH,
    params(
        ("code" = String, Path, description = "Shop code to look up")
    ),
    responses(
        (status = 200, description = "Shop details retrieved successfully", body = ApiResponse<ShopLookupResponse>),
        (status = 404, description = "Shop not found", body = ErrorResponse),
    )
)]
async fn lookup_shop(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Json<ApiResponse<ShopLookupResponse>>> {
    let response = service::lookup_shop(&state.db, &code).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Shop details retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/me", tag = modules::AUTH,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Current user info", body = ApiResponse<User>),
        (status = 401, description = "Missing/invalid/expired token, or account no longer active", body = ErrorResponse),
    )
)]
async fn me(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<User>>> {
    let current = service::me(
        &state.db,
        &user.user_id,
        user.email.as_deref(),
        user.name.as_deref(),
    )
    .await?;

    Ok(Json(ApiResponse::success(
        current,
        "Current user retrieved successfully",
    )))
}

// ============================================================================
// Login sessions
// ============================================================================

#[utoipa::path(get, path = "/sessions", tag = modules::AUTH, params(LoginSessionListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Login session history", body = ApiResponse<LoginSessionsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing sessions:view permission", body = ErrorResponse),
    )
)]
async fn list_sessions(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LoginSessionListQuery>,
) -> AppResult<Json<ApiResponse<LoginSessionsResponse>>> {
    user.require_permission(permissions::SESSIONS_VIEW)?;
    let response = service::list_sessions(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Login sessions retrieved successfully",
    )))
}
