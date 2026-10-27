// HTTP layer for the drawer rules. Anyone who can see cash movements reads them (the cash form needs
// the quick amounts and the float target); only the owner changes them.

use axum::{Json, extract::State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::cash_settings::{CashSettings, UpdateCashSettingsRequest},
    modules::cash_settings::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(get_cash_settings))
        // Writes
        .routes(routes!(update_cash_settings))
}

// ============================================================================
// Cash settings
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    responses((status = 200, description = "The shop's drawer rules", body = ApiResponse<CashSettings>)),
    security(("bearerAuth" = [])),
    tag = "Cash Settings"
)]
async fn get_cash_settings(
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
async fn update_cash_settings(
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
