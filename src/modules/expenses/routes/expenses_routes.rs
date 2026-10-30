use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::expenses::{
    CreateExpensePaymentRequest, CreateExpenseRequest, ExpenseDetail, ExpenseListQuery,
    ExpenseListResponse, ExpenseStats,
};
use crate::modules::expenses::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

#[utoipa::path(
    get,
    path = "/{key}",
    params(("key" = String, Path, description = "Expense key")),
    responses(
        (status = 200, description = "The expense with its payments", body = ApiResponse<ExpenseDetail>),
        (status = 404, description = "Expense not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn get_expense(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<ExpenseDetail>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let detail = service::get_expense(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        detail,
        "Expense retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/stats",
    responses((status = 200, description = "This month's costs and what is still owed", body = ApiResponse<ExpenseStats>)),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn get_expense_stats(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<ExpenseStats>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let stats = service::get_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Expense stats retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/",
    params(ExpenseListQuery),
    responses((status = 200, description = "Ledger expenses and drawer payments, newest first", body = ApiResponse<ExpenseListResponse>)),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn list_expenses(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<ExpenseListQuery>,
) -> AppResult<Json<ApiResponse<ExpenseListResponse>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let response = service::list_expenses(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Expenses retrieved successfully",
    )))
}

/// Routes of the expenses module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_expenses))
        .routes(routes!(get_expense_stats))
        .routes(routes!(get_expense))
        // Writes
        .routes(routes!(create_expense))
        .routes(routes!(add_expense_payment))
}
