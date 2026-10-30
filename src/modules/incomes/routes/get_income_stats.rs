use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::incomes::IncomeStats;
use crate::modules::incomes::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(
    get,
    path = "/stats",
    responses((status = 200, description = "This month's and last month's other income", body = ApiResponse<IncomeStats>)),
    security(("bearerAuth" = [])),
    tag = "Income"
)]
pub(super) async fn get_income_stats(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<IncomeStats>>> {
    current_user.require_permission(perm::INCOME_VIEW)?;
    let stats = service::get_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Income stats retrieved successfully",
    )))
}
