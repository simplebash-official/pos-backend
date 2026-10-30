use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::ApiResponse;
use crate::domain::sessions::{CashMovementListQuery, CashMovementListResponse};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/movements",
    params(CashMovementListQuery),
    responses(
        (status = 200, description = "List of cash movements", body = ApiResponse<CashMovementListResponse>)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
pub(super) async fn list_cash_movements(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<CashMovementListQuery>,
) -> AppResult<Json<ApiResponse<CashMovementListResponse>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let response = service::list_movements(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Cash movements retrieved successfully",
    )))
}
