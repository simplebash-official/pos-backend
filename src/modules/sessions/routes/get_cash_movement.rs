use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::CashMovement;

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/movements/{key}",
    params(
        ("key" = String, Path, description = "Cash movement key or number")
    ),
    responses(
        (status = 200, description = "Cash movement details", body = ApiResponse<CashMovement>),
        (status = 404, description = "Movement not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
pub(super) async fn get_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<CashMovement>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let movement = service::get_movement(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        movement,
        "Cash movement details retrieved successfully",
    )))
}
