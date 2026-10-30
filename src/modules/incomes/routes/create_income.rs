use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::incomes::{CreateIncomeRequest, IncomeRow};
use crate::modules::incomes::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/",
    request_body = CreateIncomeRequest,
    responses(
        (status = 201, description = "Income entered", body = ApiResponse<IncomeRow>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Income"
)]
pub(super) async fn create_income(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<CreateIncomeRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<IncomeRow>>)> {
    current_user.require_permission(perm::INCOME_MANAGE)?;
    let name = current_user.resolve_name(&state.db).await;
    let income =
        service::create_income(&state.db, current_user.user_id, name, body, device_id.0).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(income, "Income recorded successfully")),
    ))
}
