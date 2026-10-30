use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::recurring_expenses::{RecurringBill, RecurringExpenseListQuery};
use crate::modules::recurring_expenses::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/",
    params(RecurringExpenseListQuery),
    responses((status = 200, description = "Regular bills with where each stands this month, most urgent first", body = ApiResponse<Vec<RecurringBill>>)),
    security(("bearerAuth" = [])),
    tag = "Regular Bills"
)]
pub(super) async fn list_bills(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<RecurringExpenseListQuery>,
) -> AppResult<Json<ApiResponse<Vec<RecurringBill>>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let bills = service::list_bills(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        bills,
        "Regular bills retrieved successfully",
    )))
}
