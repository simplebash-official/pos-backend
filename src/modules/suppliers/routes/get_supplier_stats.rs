use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::suppliers::SupplierStats;
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/stats", tag = modules::SUPPLIERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Suppliers screen KPI cards: total suppliers, supply category count, and direct-contact count", body = ApiResponse<SupplierStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn get_supplier_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SupplierStats>>> {
    let stats = service::get_supplier_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Supplier stats retrieved successfully",
    )))
}
