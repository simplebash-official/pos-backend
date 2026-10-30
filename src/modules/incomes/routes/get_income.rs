use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::incomes::IncomeRow;
use crate::modules::incomes::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/{key}",
    params(("key" = String, Path, description = "Income key")),
    responses(
        (status = 200, description = "The income", body = ApiResponse<IncomeRow>),
        (status = 404, description = "Income not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Income"
)]
pub(super) async fn get_income(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<IncomeRow>>> {
    current_user.require_permission(perm::INCOME_VIEW)?;
    let income = service::get_income(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        income,
        "Income retrieved successfully",
    )))
}
