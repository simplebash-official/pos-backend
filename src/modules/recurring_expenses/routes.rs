// HTTP layer for regular bills.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::recurring_expenses::{
        CreateRecurringExpenseRequest, PeriodStatus, PeriodStatusQuery, RecurringBill,
        RecurringExpense, RecurringExpenseListQuery, UpdateRecurringExpenseRequest,
    },
    modules::recurring_expenses::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_bills))
        .routes(routes!(get_period_status))
        // Writes
        .routes(routes!(create_template))
        .routes(routes!(update_template))
}

// ============================================================================
// Regular bills
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    params(RecurringExpenseListQuery),
    responses((status = 200, description = "Regular bills with where each stands this month, most urgent first", body = ApiResponse<Vec<RecurringBill>>)),
    security(("bearerAuth" = [])),
    tag = "Regular Bills"
)]
async fn list_bills(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<RecurringExpenseListQuery>,
) -> AppResult<Json<ApiResponse<Vec<RecurringBill>>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let bills = service::list_bills(&state.db, query).await?;
    Ok(Json(ApiResponse::success(bills, "Regular bills retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "/period-status",
    params(PeriodStatusQuery),
    responses((status = 200, description = "What is already recorded for this category and payee in the month", body = ApiResponse<PeriodStatus>)),
    security(("bearerAuth" = [])),
    tag = "Regular Bills"
)]
async fn get_period_status(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PeriodStatusQuery>,
) -> AppResult<Json<ApiResponse<PeriodStatus>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let status = service::period_status(&state.db, query).await?;
    Ok(Json(ApiResponse::success(status, "Period status retrieved successfully")))
}

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
async fn create_template(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreateRecurringExpenseRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<RecurringExpense>>)> {
    current_user.require_permission(perm::EXPENSES_MANAGE)?;
    let template = service::create_template(&state.db, body).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::success(template, "Regular bill added successfully"))))
}

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
async fn update_template(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<UpdateRecurringExpenseRequest>,
) -> AppResult<Json<ApiResponse<RecurringExpense>>> {
    current_user.require_permission(perm::EXPENSES_MANAGE)?;
    let template = service::update_template(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(template, "Regular bill updated successfully")))
}
