use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::balances::StaffAdvancesResponse;
use crate::modules::balances::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(
    get,
    path = "/staff",
    responses((status = 200, description = "Staff who have been given an advance, with what they still owe", body = ApiResponse<StaffAdvancesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
pub(super) async fn list_staff_advances(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<StaffAdvancesResponse>>> {
    current_user.require_permission(perm::BALANCES_VIEW)?;
    let response = service::list_staff_advances(&state.db).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Staff advances retrieved successfully",
    )))
}
