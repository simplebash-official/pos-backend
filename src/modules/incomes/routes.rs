// HTTP layer for the income ledger.

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
        middleware::{auth::CurrentUser, sync_headers::DeviceId},
        response::{ApiResponse, ErrorResponse},
    },
    domain::incomes::{
        CreateIncomeRequest, IncomeListQuery, IncomeListResponse, IncomeRow, IncomeStats,
    },
    modules::incomes::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_incomes))
        .routes(routes!(get_income_stats))
        .routes(routes!(get_income))
        // Writes
        .routes(routes!(create_income))
}

// ============================================================================
// Income
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    params(IncomeListQuery),
    responses((status = 200, description = "Ledger income and drawer additions, newest first", body = ApiResponse<IncomeListResponse>)),
    security(("bearerAuth" = [])),
    tag = "Income"
)]
async fn list_incomes(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<IncomeListQuery>,
) -> AppResult<Json<ApiResponse<IncomeListResponse>>> {
    current_user.require_permission(perm::INCOME_VIEW)?;
    let response = service::list_incomes(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Income retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/stats",
    responses((status = 200, description = "This month's and last month's other income", body = ApiResponse<IncomeStats>)),
    security(("bearerAuth" = [])),
    tag = "Income"
)]
async fn get_income_stats(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<IncomeStats>>> {
    current_user.require_permission(perm::INCOME_VIEW)?;
    let stats = service::get_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Income stats retrieved successfully",
    )))
}

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
async fn get_income(
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
async fn create_income(
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
