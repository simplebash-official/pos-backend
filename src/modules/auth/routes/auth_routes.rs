use crate::app::AppState;
use crate::core::constants::{codes, modules, permissions};
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::auth::{
    LoginRequest, LoginResponse, LoginSessionListQuery, LoginSessionsResponse, ShopLookupResponse,
};
use crate::domain::sessions::{SetPinRequest, VerifyPinRequest, VerifyPinResponse};
use crate::domain::users::{Role, UpdateMyPreferencesRequest, User};
use crate::modules::auth::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/sessions", tag = modules::AUTH, params(LoginSessionListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Login session history", body = ApiResponse<LoginSessionsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing sessions:view permission", body = ErrorResponse),
    )
)]
pub(super) async fn list_sessions(
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
        (status = 409, description = "SESSION_OPEN_ON_OTHER_DEVICE: a staff account whose shift is open on another machine (X-Device-Id)", body = ErrorResponse),
    )
)]
pub(super) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    device_id: DeviceId,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    let ip_address = extract_ip_address(&headers);
    let user_agent = extract_user_agent(&headers);

    let response = service::login(
        &state.db,
        &state.config,
        body,
        ip_address,
        user_agent,
        device_id.0,
    )
    .await?;

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
pub(super) async fn lookup_shop(
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
pub(super) async fn me(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<User>>> {
    let current = service::me(&state.db, &user.user_id, user.name.as_deref()).await?;

    Ok(Json(ApiResponse::success(
        current,
        "Current user retrieved successfully",
    )))
}

/// Routes of the auth module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(lookup_shop))
        .routes(routes!(me))
        .routes(routes!(update_my_preferences))
        .routes(routes!(list_sessions))
        .routes(routes!(verify_pin))
        .routes(routes!(set_pin))
}

#[utoipa::path(post, path = "/set-pin", tag = modules::AUTH, request_body = SetPinRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "PIN set successfully", body = ApiResponse<bool>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden: only admins can set PIN for other users", body = ErrorResponse),
    )
)]
pub(super) async fn set_pin(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<SetPinRequest>,
) -> AppResult<Json<ApiResponse<bool>>> {
    let target_user_id = match &body.target_user_id {
        Some(id) if !id.trim().is_empty() => {
            if id != &user.user_id && user.role != Some(Role::Admin) {
                return Err(AppError::forbidden(
                    "Only admins can set PIN for other users",
                ));
            }
            id.as_str()
        }
        _ => &user.user_id,
    };

    crate::modules::users::service::set_user_pin(&state.db, target_user_id, &body.pin).await?;
    Ok(Json(ApiResponse::success(true, "PIN set successfully")))
}

#[utoipa::path(patch, path = "/me/preferences", tag = modules::AUTH,
    request_body = UpdateMyPreferencesRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Updated current user (with preferences)", body = ApiResponse<User>),
        (status = 400, description = "Unknown preference value", body = ErrorResponse),
        (status = 401, description = "Missing/invalid/expired token, or account no longer active", body = ErrorResponse),
    )
)]
pub(super) async fn update_my_preferences(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateMyPreferencesRequest>,
) -> AppResult<Json<ApiResponse<User>>> {
    let updated =
        service::update_my_preferences(&state.db, &user.user_id, user.name.as_deref(), body)
            .await?;

    Ok(Json(ApiResponse::success(
        updated,
        "Preferences updated successfully",
    )))
}

#[utoipa::path(post, path = "/verify-pin", tag = modules::AUTH, request_body = VerifyPinRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "PIN verification successful", body = ApiResponse<VerifyPinResponse>),
        (status = 401, description = "Invalid PIN or insufficient permissions", body = ErrorResponse),
    )
)]
pub(super) async fn verify_pin(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<VerifyPinRequest>,
) -> AppResult<Json<ApiResponse<VerifyPinResponse>>> {
    let verified_user = crate::modules::users::service::verify_user_pin(
        &state.db,
        &body.pin,
        body.required_role.as_deref(),
    )
    .await?;

    match verified_user {
        Some(user) => Ok(Json(ApiResponse::success(
            VerifyPinResponse {
                valid: true,
                user_id: Some(user.id),
                user_name: Some(user.name),
                role: Some(user.role.as_str().to_string()),
            },
            "PIN verified successfully",
        ))),
        None => Err(AppError::unauthorized_with_code(
            codes::INVALID_PIN,
            "Invalid PIN or insufficient role permissions",
        )),
    }
}
