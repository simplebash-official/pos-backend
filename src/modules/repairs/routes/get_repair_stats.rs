use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::RepairStats;
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/stats", tag = modules::REPAIRS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repair Jobs KPI cards: today's job count, today's revenue, open ticket count, and average job value", body = ApiResponse<RepairStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_repair_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<RepairStats>>> {
    let stats = service::get_repair_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Repair stats retrieved successfully",
    )))
}
