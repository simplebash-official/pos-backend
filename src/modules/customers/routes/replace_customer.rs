use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::{DeviceId, IfMatch};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::{CreateCustomerRequest, Customer};
use crate::modules::customers::service;
use axum::Json;
use axum::extract::{Path, State};

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
pub(super) async fn replace_customer(
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
