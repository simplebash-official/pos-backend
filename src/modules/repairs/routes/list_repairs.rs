use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::{RepairListQuery, RepairListResponse};
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::REPAIRS, params(RepairListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List repair tickets with search, status filtering, and pagination", body = ApiResponse<RepairListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_repairs(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<RepairListQuery>,
) -> AppResult<Json<ApiResponse<RepairListResponse>>> {
    let response = service::list_repairs(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Repair tickets retrieved successfully",
    )))
}
