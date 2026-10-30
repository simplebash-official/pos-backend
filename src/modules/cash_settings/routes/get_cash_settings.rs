use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::cash_settings::CashSettings;
use crate::modules::cash_settings::service;
use axum::Json;
use axum::extract::State;

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
