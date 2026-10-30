use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::recurring_expenses::{CreateRecurringExpenseRequest, RecurringExpense};
use crate::modules::recurring_expenses::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/",
    request_body = CreateRecurringExpenseRequest,
    responses(
        (status = 201, description = "Regular bill added", body = ApiResponse<RecurringExpense>),
        (status = 409, description = "There is already one for this category and payee", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Regular Bills"
)]
pub(super) async fn create_template(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreateRecurringExpenseRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<RecurringExpense>>)> {
    current_user.require_permission(perm::EXPENSES_MANAGE)?;
    let template = service::create_template(&state.db, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            template,
            "Regular bill added successfully",
        )),
    ))
}
