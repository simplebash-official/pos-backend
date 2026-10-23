// HTTP layer for supplier balances and staff advances.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::{AppError, AppResult},
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::balances::{
        BalanceSubject, CreateAdvanceRecoveryRequest, SetBalanceOpeningRequest, StaffAdvanceDetail,
        StaffAdvancesResponse, SupplierBalancesResponse,
    },
    modules::balances::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_supplier_balances))
        .routes(routes!(list_staff_advances))
        .routes(routes!(get_staff_advance))
        // Writes
        .routes(routes!(set_balance_opening))
        .routes(routes!(recover_advance))
}

// ============================================================================
// Supplier balances
// ============================================================================

#[utoipa::path(
    get,
    path = "/suppliers",
    responses((status = 200, description = "Every supplier with what the shop owes them", body = ApiResponse<SupplierBalancesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
async fn list_supplier_balances(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<SupplierBalancesResponse>>> {
    current_user.require_permission(perm::BALANCES_VIEW)?;
    let response = service::list_supplier_balances(&state.db).await?;
    Ok(Json(ApiResponse::success(response, "Supplier balances retrieved successfully")))
}

// ============================================================================
// Staff advances
// ============================================================================

#[utoipa::path(
    get,
    path = "/staff",
    responses((status = 200, description = "Staff who have been given an advance, with what they still owe", body = ApiResponse<StaffAdvancesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
async fn list_staff_advances(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<StaffAdvancesResponse>>> {
    current_user.require_permission(perm::BALANCES_VIEW)?;
    let response = service::list_staff_advances(&state.db).await?;
    Ok(Json(ApiResponse::success(response, "Staff advances retrieved successfully")))
}

#[utoipa::path(
    get,
    path = "/staff/{employeeKey}",
    params(("employeeKey" = String, Path, description = "Employee key")),
    responses(
        (status = 200, description = "One staff member's advances given and recovered", body = ApiResponse<StaffAdvanceDetail>),
        (status = 404, description = "Employee not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
async fn get_staff_advance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(employee_key): Path<String>,
) -> AppResult<Json<ApiResponse<StaffAdvanceDetail>>> {
    current_user.require_permission(perm::BALANCES_VIEW)?;
    let detail = service::get_staff_advance(&state.db, &employee_key).await?;
    Ok(Json(ApiResponse::success(detail, "Staff advance retrieved successfully")))
}

#[utoipa::path(
    post,
    path = "/staff/{employeeKey}/recoveries",
    params(("employeeKey" = String, Path, description = "Employee key")),
    request_body = CreateAdvanceRecoveryRequest,
    responses(
        (status = 201, description = "Recovery recorded", body = ApiResponse<StaffAdvanceDetail>),
        (status = 409, description = "More than the staff member still owes", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
async fn recover_advance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(employee_key): Path<String>,
    Json(body): Json<CreateAdvanceRecoveryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<StaffAdvanceDetail>>)> {
    current_user.require_permission(perm::BALANCES_MANAGE)?;
    let name = current_user.resolve_name(&state.db).await;
    let detail = service::recover_advance(&state.db, &employee_key, current_user.user_id, name, body).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::success(detail, "Recovery recorded successfully"))))
}

// ============================================================================
// Openings
// ============================================================================

#[utoipa::path(
    put,
    path = "/{kind}/{key}/opening",
    params(
        ("kind" = String, Path, description = "supplier | staff"),
        ("key" = String, Path, description = "Supplier or employee key")
    ),
    request_body = SetBalanceOpeningRequest,
    responses(
        (status = 200, description = "Opening balance saved", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
async fn set_balance_opening(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path((kind, key)): Path<(String, String)>,
    Json(body): Json<SetBalanceOpeningRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    current_user.require_permission(perm::BALANCES_MANAGE)?;
    let subject = BalanceSubject::parse(&kind).ok_or_else(|| AppError::validation("Unknown kind."))?;
    let name = current_user.resolve_name(&state.db).await;
    service::set_opening(&state.db, subject, &key, name, body).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({}), "Opening balance saved successfully")))
}
