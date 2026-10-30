use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::suppliers::{CreateSupplierRequest, Supplier};
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/", tag = modules::SUPPLIERS, request_body = CreateSupplierRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Supplier created", body = ApiResponse<Supplier>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing the suppliers:create permission", body = ErrorResponse),
    )
)]
pub(super) async fn create_supplier(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSupplierRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Supplier>>)> {
    current_user.require_permission(perm::SUPPLIERS_CREATE)?;
    let supplier = service::create_supplier(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            supplier,
            "Supplier created successfully",
        )),
    ))
}
