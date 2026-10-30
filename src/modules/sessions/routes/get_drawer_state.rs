use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::ApiResponse;
use crate::domain::sessions::DrawerState;

use crate::modules::sessions::repository::drawer;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/drawer-state/{terminal_id}",
    params(
        ("terminal_id" = String, Path, description = "Terminal identifier")
    ),
    responses(
        (status = 200, description = "Current drawer ledger state", body = ApiResponse<Option<DrawerState>>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_drawer_state(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(terminal_id): Path<String>,
) -> AppResult<Json<ApiResponse<Option<DrawerState>>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let drawer_state = drawer::get_drawer_state(&state.db, &terminal_id).await?;
    Ok(Json(ApiResponse::success(
        drawer_state.map(|s| s.into_drawer_state()),
        "Drawer state retrieved successfully",
    )))
}
