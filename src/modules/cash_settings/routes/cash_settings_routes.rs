use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_settings::{CashSettings, UpdateCashSettingsRequest};
use crate::modules::cash_settings::service;
use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(
    get,
    path = "/",
    responses((status = 200, description = "The shop's drawer rules", body = ApiResponse<CashSettings>)),
    security(("bearerAuth" = [])),
    tag = "Cash Settings"
)]
pub(super) async fn get_cash_settings(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<CashSettings>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let settings = service::get_settings(&state.db).await?;
    Ok(Json(ApiResponse::success(
        settings,
        "Cash settings retrieved successfully",
    )))
}

/// Routes of the cash settings module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(get_cash_settings))
        // Writes
        .routes(routes!(update_cash_settings))
}

#[utoipa::path(
    put,
    path = "/",
    request_body = UpdateCashSettingsRequest,
    responses(
        (status = 200, description = "The new drawer rules", body = ApiResponse<CashSettings>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Settings"
)]
pub(super) async fn update_cash_settings(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<UpdateCashSettingsRequest>,
) -> AppResult<Json<ApiResponse<CashSettings>>> {
    current_user.require_permission(perm::CASH_SETTINGS_MANAGE)?;
    let name = current_user.resolve_name(&state.db).await;
    let settings = service::update_settings(&state.db, name, body).await?;
    Ok(Json(ApiResponse::success(
        settings,
        "Cash settings saved successfully",
    )))
}
