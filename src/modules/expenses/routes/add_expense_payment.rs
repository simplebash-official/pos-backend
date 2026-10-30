use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::expenses::{CreateExpensePaymentRequest, ExpenseDetail};
use crate::modules::expenses::service;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/{key}/payments",
    params(("key" = String, Path, description = "Expense key")),
    request_body = CreateExpensePaymentRequest,
    responses(
        (status = 201, description = "Payment recorded", body = ApiResponse<ExpenseDetail>),
        (status = 409, description = "More than is still owed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn add_expense_payment(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<CreateExpensePaymentRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<ExpenseDetail>>)> {
    current_user.require_permission(perm::EXPENSES_MANAGE)?;
    let name = current_user.resolve_name(&state.db).await;
    let detail = service::add_payment(&state.db, &key, current_user.user_id, name, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            detail,
            "Payment recorded successfully",
        )),
    ))
}
