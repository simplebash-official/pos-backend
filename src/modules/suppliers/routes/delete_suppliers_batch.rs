use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::suppliers::{DeleteSuppliersRequest, DeleteSuppliersResponse};
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(delete, path = "/batch", tag = modules::SUPPLIERS, request_body = DeleteSuppliersRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Suppliers deleted", body = ApiResponse<DeleteSuppliersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn delete_suppliers_batch(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<DeleteSuppliersRequest>,
) -> AppResult<Json<ApiResponse<DeleteSuppliersResponse>>> {
    let deleted_count = service::delete_suppliers(&state.db, body.ids).await?;

    Ok(Json(ApiResponse::success(
        DeleteSuppliersResponse { deleted_count },
        format!("{deleted_count} suppliers deleted successfully"),
    )))
}
