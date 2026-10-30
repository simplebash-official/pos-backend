use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::{DeleteCustomersRequest, DeleteCustomersResponse};
use crate::modules::customers::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(delete, path = "/batch", tag = modules::CUSTOMERS,
    request_body = DeleteCustomersRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Batch customers deleted", body = ApiResponse<DeleteCustomersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn delete_customers_batch(
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
