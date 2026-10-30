use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::recurring_expenses::{RecurringExpense, UpdateRecurringExpenseRequest};
use crate::modules::recurring_expenses::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    patch,
    path = "/{key}",
    params(("key" = String, Path, description = "Regular bill key")),
    request_body = UpdateRecurringExpenseRequest,
    responses(
        (status = 200, description = "Regular bill updated", body = ApiResponse<RecurringExpense>),
        (status = 404, description = "Regular bill not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Regular Bills"
)]
pub(super) async fn update_template(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<UpdateRecurringExpenseRequest>,
) -> AppResult<Json<ApiResponse<RecurringExpense>>> {
    current_user.require_permission(perm::EXPENSES_MANAGE)?;
    let template = service::update_template(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        template,
        "Regular bill updated successfully",
    )))
}
