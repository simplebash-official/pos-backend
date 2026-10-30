use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::expenses::{CreateExpenseRequest, ExpenseDetail};
use crate::modules::expenses::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/",
    request_body = CreateExpenseRequest,
    responses(
        (status = 201, description = "Expense entered", body = ApiResponse<ExpenseDetail>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn create_expense(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<CreateExpenseRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<ExpenseDetail>>)> {
    current_user.require_permission(perm::EXPENSES_MANAGE)?;
    let name = current_user.resolve_name(&state.db).await;
    let detail =
        service::create_expense(&state.db, current_user.user_id, name, body, device_id.0).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            detail,
            "Expense recorded successfully",
        )),
    ))
}
