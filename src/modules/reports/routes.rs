// HTTP layer for reports and financial intelligence: executive dashboard KPIs, daily sales,
// monthly profit/loss, credit receivables aging, employee commission payouts, inventory valuation,
// and top selling products.

use axum::{
    Json,
    extract::{Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions as perm},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
        utils::module_status_response,
    },
    domain::{
        ModuleStatusResponse,
        reports::{
            DailySalesQuery, DailySalesReportResponse, EmployeeCommissionsQuery,
            EmployeeCommissionsReportResponse, InventoryValuationResponse, MonthlyProfitQuery,
            MonthlyProfitReportResponse, OutstandingReceivablesQuery,
            OutstandingReceivablesResponse, ReportDateRangeQuery, ReportsDashboardResponse,
            TopProductsQuery, TopProductsResponse,
        },
    },
    modules::reports::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Module status
        .routes(routes!(status))
        // Executive overview & KPIs
        .routes(routes!(get_dashboard_overview))
        // Sales & Profit intelligence
        .routes(routes!(get_daily_sales))
        .routes(routes!(get_monthly_profit))
        // Receivables & Outstanding credit
        .routes(routes!(get_outstanding_receivables))
        // Employee commissions & performance
        .routes(routes!(get_employee_commissions))
        // Inventory valuation & Product performance
        .routes(routes!(get_inventory_valuation))
        .routes(routes!(get_top_products))
}

// ============================================================================
// Module status
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    tag = modules::REPORTS,
    responses(
        (status = 200, description = "Reports module status", body = ApiResponse<ModuleStatusResponse>)
    )
)]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::REPORTS)
}

// ============================================================================
// Executive Dashboard
// ============================================================================

#[utoipa::path(
    get,
    path = "/dashboard",
    tag = modules::REPORTS,
    params(ReportDateRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Reports & Profit Intelligence executive dashboard overview", body = ApiResponse<ReportsDashboardResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_dashboard_overview(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ReportDateRangeQuery>,
) -> AppResult<Json<ApiResponse<ReportsDashboardResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_dashboard_overview(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Dashboard overview report retrieved successfully",
    )))
}

// ============================================================================
// Sales & Profit Reports
// ============================================================================

#[utoipa::path(
    get,
    path = "/daily-sales",
    tag = modules::REPORTS,
    params(DailySalesQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Daily sales report with stream and payment method breakdowns", body = ApiResponse<DailySalesReportResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_daily_sales(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<DailySalesQuery>,
) -> AppResult<Json<ApiResponse<DailySalesReportResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_daily_sales_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Daily sales report retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/monthly-profit",
    tag = modules::REPORTS,
    params(MonthlyProfitQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Monthly profit report with revenue, COGS, commissions, and refunds", body = ApiResponse<MonthlyProfitReportResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_monthly_profit(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<MonthlyProfitQuery>,
) -> AppResult<Json<ApiResponse<MonthlyProfitReportResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_monthly_profit_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Monthly profit report retrieved successfully",
    )))
}

// ============================================================================
// Receivables & Outstanding Credit
// ============================================================================

#[utoipa::path(
    get,
    path = "/outstanding",
    tag = modules::REPORTS,
    params(OutstandingReceivablesQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Outstanding credit receivables report with overdue aging", body = ApiResponse<OutstandingReceivablesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_outstanding_receivables(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<OutstandingReceivablesQuery>,
) -> AppResult<Json<ApiResponse<OutstandingReceivablesResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_outstanding_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Outstanding receivables report retrieved successfully",
    )))
}

// ============================================================================
// Employee Commissions & Performance
// ============================================================================

#[utoipa::path(
    get,
    path = "/employee-commissions",
    tag = modules::REPORTS,
    params(EmployeeCommissionsQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Employee commission payouts and technician job performance report", body = ApiResponse<EmployeeCommissionsReportResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_employee_commissions(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<EmployeeCommissionsQuery>,
) -> AppResult<Json<ApiResponse<EmployeeCommissionsReportResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_employee_commissions_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Employee commissions report retrieved successfully",
    )))
}

// ============================================================================
// Inventory Valuation & Top Products
// ============================================================================

#[utoipa::path(
    get,
    path = "/inventory-valuation",
    tag = modules::REPORTS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Inventory valuation report at cost vs retail price", body = ApiResponse<InventoryValuationResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_inventory_valuation(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<InventoryValuationResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_inventory_valuation(&state.db).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Inventory valuation report retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/top-products",
    tag = modules::REPORTS,
    params(TopProductsQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Top selling products report by volume or revenue", body = ApiResponse<TopProductsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_top_products(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<TopProductsQuery>,
) -> AppResult<Json<ApiResponse<TopProductsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_top_products(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Top products report retrieved successfully",
    )))
}
