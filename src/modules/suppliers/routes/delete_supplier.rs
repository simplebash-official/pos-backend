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

#[utoipa::path(delete, path = "/{id}", tag = modules::SUPPLIERS,
    params(("id" = String, Path, description = "Supplier id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier deleted", body = ApiResponse<Supplier>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier not found", body = ErrorResponse),
        (status = 409, description = "Supplier is still referenced by purchase history or version conflict", body = ErrorResponse),
    )
)]
pub(super) async fn delete_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Supplier>>> {
    let object_id = parse_object_id(&id)?;
    let supplier = service::delete_supplier(&state.db, object_id, if_match.0, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        supplier,
        "Supplier deleted successfully",
    )))
}
