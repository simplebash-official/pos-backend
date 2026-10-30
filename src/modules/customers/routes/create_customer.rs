use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::{CreateCustomerRequest, Customer};
use crate::modules::customers::service;
use axum::Json;
use axum::extract::State;

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
pub(super) async fn create_customer(
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
