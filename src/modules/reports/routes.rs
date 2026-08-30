// HTTP layer for reports and financial intelligence: executive dashboard KPIs, daily sales,
// monthly profit/loss, credit receivables aging, employee commission payouts, inventory valuation,
// and top selling products.

use axum::{
    Json,
    body::Body,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
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
        employees::EmployeeEarningsResponse,
        reports::{
            AnalyticsPaymentMethodsResponse, AnalyticsRangeQuery, AnalyticsSummaryResponse,
            CashierPerformanceResponse, DailySalesQuery, DailySalesReportResponse,
            DiscountAnalyticsResponse, EmployeeCommissionsQuery, EmployeeCommissionsReportResponse,
            InventoryValuationResponse, MonthlyProfitQuery, MonthlyProfitReportResponse,
            OutstandingReceivablesQuery, OutstandingReceivablesResponse, ReceivablesAgingQuery,
            ReceivablesAgingResponse, RefundAnalyticsResponse, ReportDateRangeQuery,
            ReportsDashboardResponse, SalesByCategoryQuery, SalesByCategoryResponse,
            SalesPatternsResponse, TimeSeriesResponse, TopCustomersQuery, TopCustomersResponse,
            TopProductsQuery, TopProductsResponse,
        },
    },
    modules::documents,
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
        .routes(routes!(get_employee_earnings_items))
        // Inventory valuation & Product performance
        .routes(routes!(get_inventory_valuation))
        .routes(routes!(get_top_products))
        // Analytics & Reports
        .routes(routes!(get_analytics_summary))
        .routes(routes!(get_analytics_timeseries))
        .routes(routes!(get_analytics_payment_methods))
        .routes(routes!(get_analytics_top_customers))
        .routes(routes!(get_analytics_sales_by_category))
        .routes(routes!(get_analytics_cashier_performance))
        .routes(routes!(get_analytics_sales_patterns))
        .routes(routes!(get_analytics_receivables_aging))
        .routes(routes!(get_analytics_discounts))
        .routes(routes!(get_analytics_refunds))
        .routes(routes!(get_analytics_document))
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

#[utoipa::path(
    get,
    path = "/employee-commissions/{employeeKey}/items",
    tag = modules::REPORTS,
    params(("employeeKey" = String, Path, description = "Employee key"), EmployeeCommissionsQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Itemized commission line items for one employee", body = ApiResponse<EmployeeEarningsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Employee not found", body = ErrorResponse),
    )
)]
async fn get_employee_earnings_items(
    user: CurrentUser,
    State(state): State<AppState>,
    axum::extract::Path(employee_key): axum::extract::Path<String>,
    Query(query): Query<EmployeeCommissionsQuery>,
) -> AppResult<Json<ApiResponse<EmployeeEarningsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_employee_earnings(&state.db, &employee_key, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Employee earnings retrieved successfully",
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

// ============================================================================
// Analytics & Reports
// ============================================================================

#[utoipa::path(
    get,
    path = "/analytics/summary",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Headline analytics KPIs with optional previous-period comparison", body = ApiResponse<AnalyticsSummaryResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_summary(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<AnalyticsSummaryResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_summary(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics summary retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/timeseries",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Gap-filled revenue/profit time series bucketed by day, week, month or year", body = ApiResponse<TimeSeriesResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_timeseries(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<TimeSeriesResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_timeseries(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics time series retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/payment-methods",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Payment-method split (cash/card/online/credit) over the range", body = ApiResponse<AnalyticsPaymentMethodsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_payment_methods(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<AnalyticsPaymentMethodsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_payment_methods(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics payment methods retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/top-customers",
    tag = modules::REPORTS,
    params(TopCustomersQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Top customers by revenue, invoices or product margin", body = ApiResponse<TopCustomersResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_top_customers(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<TopCustomersQuery>,
) -> AppResult<Json<ApiResponse<TopCustomersResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_top_customers(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics top customers retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/sales-by-category",
    tag = modules::REPORTS,
    params(SalesByCategoryQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Retail sales rolled up by product category or subcategory", body = ApiResponse<SalesByCategoryResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_sales_by_category(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SalesByCategoryQuery>,
) -> AppResult<Json<ApiResponse<SalesByCategoryResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_sales_by_category(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics sales by category retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/cashier-performance",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Per-cashier sales, discounts, refunds and product margin", body = ApiResponse<CashierPerformanceResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_cashier_performance(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<CashierPerformanceResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_cashier_performance(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics cashier performance retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/sales-patterns",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Weekday × hour sales heatmap in shop-local time", body = ApiResponse<SalesPatternsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_sales_patterns(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<SalesPatternsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_sales_patterns(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics sales patterns retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/receivables-aging",
    tag = modules::REPORTS,
    params(ReceivablesAgingQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Open credit invoices bucketed by age (current / 1-30 / 31-60 / 61-90 / 90+)", body = ApiResponse<ReceivablesAgingResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_receivables_aging(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ReceivablesAgingQuery>,
) -> AppResult<Json<ApiResponse<ReceivablesAgingResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_receivables_aging(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics receivables aging retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/discounts",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Discount totals, split by type/cashier/product", body = ApiResponse<DiscountAnalyticsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_discounts(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<DiscountAnalyticsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_discounts(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics discounts retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/refunds",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Refund / returns analytics from non-voided credit notes", body = ApiResponse<RefundAnalyticsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_refunds(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<RefundAnalyticsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_refunds(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics refunds retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/analytics/document",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rendered analytics PDF report", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn get_analytics_document(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Response> {
    user.require_permission(perm::REPORTS_VIEW)?;
    // PDF only — CSV export is done entirely client-side from the JSON
    // endpoints, so there is no `format` param here.
    let payload = service::build_analytics_report_data(&state.db, query).await?;
    // Rendered fresh every time (no `get_or_render` cache): a report over a
    // still-open period changes with every sale and has no owning entity.
    let pdf_bytes =
        documents::service::render_uncached(&state.document_server, "analytics-report", payload)
            .await?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"analytics-report.pdf\"",
            ),
        ],
        Body::from(pdf_bytes),
    )
        .into_response())
}
