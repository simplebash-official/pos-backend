use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::Repair;
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{id}", tag = modules::REPAIRS,
    params(("id" = String, Path, description = "Repair ticket ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a repair ticket", body = ApiResponse<Repair>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Repair ticket not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_repair(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Repair>>> {
    let repair = service::get_repair(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        repair,
        "Repair ticket retrieved successfully",
    )))
}
