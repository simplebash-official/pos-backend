use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::suppliers::{SupplierListQuery, SuppliersResponse};
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::SUPPLIERS, params(SupplierListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List suppliers", body = ApiResponse<SuppliersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_suppliers(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<SupplierListQuery>,
) -> AppResult<Json<ApiResponse<SuppliersResponse>>> {
    let response = service::list_suppliers(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Suppliers retrieved successfully",
    )))
}
