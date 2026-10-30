use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::recurring_expenses::{PeriodStatus, PeriodStatusQuery};
use crate::modules::recurring_expenses::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/period-status",
    params(PeriodStatusQuery),
    responses((status = 200, description = "What is already recorded for this category and payee in the month", body = ApiResponse<PeriodStatus>)),
    security(("bearerAuth" = [])),
    tag = "Regular Bills"
)]
pub(super) async fn get_period_status(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PeriodStatusQuery>,
) -> AppResult<Json<ApiResponse<PeriodStatus>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let status = service::period_status(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        status,
        "Period status retrieved successfully",
    )))
}
