use super::parse_object_id::parse_object_id;
use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::suppliers::Supplier;
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{id}", tag = modules::SUPPLIERS,
    params(("id" = String, Path, description = "Supplier id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a supplier", body = ApiResponse<Supplier>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Supplier>>> {
    let object_id = parse_object_id(&id)?;
    let supplier = service::get_supplier(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        supplier,
        "Supplier retrieved successfully",
    )))
}
