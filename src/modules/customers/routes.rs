// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — the business logic lives in `service/`.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions as perm},
        error::AppResult,
        middleware::{
            auth::CurrentUser,
            sync_headers::{DeviceId, IfMatch},
        },
        response::{ApiResponse, ErrorResponse},
    },
    domain::customers::{
        CreateCustomerRequest, Customer, CustomerListQuery, CustomerListResponse, CustomerStats,
        CustomerTagsResponse, DeleteCustomersRequest, DeleteCustomersResponse,
        UpdateCustomerRequest,
    },
    modules::customers::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Customers
        .routes(routes!(list_customers, create_customer))
        .routes(routes!(delete_customers_batch))
        .routes(routes!(get_customer_tags))
        .routes(routes!(get_customer_stats))
        .routes(routes!(
            get_customer,
            replace_customer,
            update_customer,
            delete_customer
        ))
}

// ============================================================================
// Customers
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::CUSTOMERS, params(CustomerListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List customers with search, tag filtration, and pagination", body = ApiResponse<CustomerListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn list_customers(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<CustomerListQuery>,
) -> AppResult<Json<ApiResponse<CustomerListResponse>>> {
    let response = service::list_customers(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Customers retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/tags", tag = modules::CUSTOMERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Distinct customer tags in use", body = ApiResponse<CustomerTagsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn get_customer_tags(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<CustomerTagsResponse>>> {
    let response = service::get_customer_tags(&state.db).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Customer tags retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/stats", tag = modules::CUSTOMERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Customers screen KPI cards: total customers, total balance due, and active-debtor count", body = ApiResponse<CustomerStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn get_customer_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<CustomerStats>>> {
    let stats = service::get_customer_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Customer stats retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/{id}", tag = modules::CUSTOMERS,
    params(("id" = String, Path, description = "Customer ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get single customer profile", body = ApiResponse<Customer>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Customer not found", body = ErrorResponse),
    )
)]
async fn get_customer(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Customer>>> {
    let customer = service::get_customer(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        customer,
        "Customer retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/", tag = modules::CUSTOMERS,
    request_body = CreateCustomerRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Customer created successfully", body = ApiResponse<Customer>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn create_customer(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: DeviceId,
    Json(body): Json<CreateCustomerRequest>,
) -> AppResult<Json<ApiResponse<Customer>>> {
    user.require_permission(perm::CUSTOMERS_WRITE)?;
    let customer = service::create_customer(&state.db, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        customer,
        "Customer created successfully",
    )))
}

#[utoipa::path(put, path = "/{id}", tag = modules::CUSTOMERS,
    params(("id" = String, Path, description = "Customer ID or prefixed key")),
    request_body = CreateCustomerRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Customer updated successfully", body = ApiResponse<Customer>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Customer not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
async fn replace_customer(
    user: CurrentUser,
    State(state): State<AppState>,
    if_match: IfMatch,
    device_id: DeviceId,
    Path(id): Path<String>,
    Json(body): Json<CreateCustomerRequest>,
) -> AppResult<Json<ApiResponse<Customer>>> {
    user.require_permission(perm::CUSTOMERS_WRITE)?;
    let customer = service::replace_customer(&state.db, &id, body, if_match.0, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        customer,
        "Customer updated successfully",
    )))
}

#[utoipa::path(patch, path = "/{id}", tag = modules::CUSTOMERS,
    params(("id" = String, Path, description = "Customer ID or prefixed key")),
    request_body = UpdateCustomerRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Customer partially updated", body = ApiResponse<Customer>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Customer not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
async fn update_customer(
    user: CurrentUser,
    State(state): State<AppState>,
    if_match: IfMatch,
    device_id: DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdateCustomerRequest>,
) -> AppResult<Json<ApiResponse<Customer>>> {
    user.require_permission(perm::CUSTOMERS_WRITE)?;
    let customer = service::update_customer(&state.db, &id, body, if_match.0, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        customer,
        "Customer updated successfully",
    )))
}

#[utoipa::path(delete, path = "/{id}", tag = modules::CUSTOMERS,
    params(("id" = String, Path, description = "Customer ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Customer deleted successfully", body = ApiResponse<Customer>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Customer not found", body = ErrorResponse),
        (status = 409, description = "Customer has outstanding balance or version conflict", body = ErrorResponse),
    )
)]
async fn delete_customer(
    user: CurrentUser,
    State(state): State<AppState>,
    if_match: IfMatch,
    device_id: DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Customer>>> {
    user.require_permission(perm::CUSTOMERS_WRITE)?;
    let customer = service::delete_customer(&state.db, &id, if_match.0, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        customer,
        "Customer deleted successfully",
    )))
}

#[utoipa::path(delete, path = "/batch", tag = modules::CUSTOMERS,
    request_body = DeleteCustomersRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Batch customers deleted", body = ApiResponse<DeleteCustomersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn delete_customers_batch(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<DeleteCustomersRequest>,
) -> AppResult<Json<ApiResponse<DeleteCustomersResponse>>> {
    user.require_permission(perm::CUSTOMERS_WRITE)?;
    let deleted_count = service::delete_customers(&state.db, body.ids).await?;
    let message = format!("{deleted_count} Customers deleted successfully");
    Ok(Json(ApiResponse::success(
        DeleteCustomersResponse { deleted_count },
        message,
    )))
}
