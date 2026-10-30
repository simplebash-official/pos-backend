use super::current_session_query::CurrentSessionQuery;
use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::ApiResponse;
use crate::domain::sessions::RegisterSession;

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/current",
    params(
        ("terminal_id" = Option<String>, Query, description = "Terminal ID to check active session for")
    ),
    responses(
        (status = 200, description = "Current active session for terminal or cashier", body = ApiResponse<Option<RegisterSession>>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_current_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Query(query): Query<CurrentSessionQuery>,
) -> AppResult<Json<ApiResponse<Option<RegisterSession>>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;

    let terminal_id = query.terminal_id.as_deref().or(device_id.0.as_deref());
    let mut session =
        service::resolve_active_session(&state.db, terminal_id, &current_user.user_id)
            .await?
            .map(|s| s.into_register_session());

    if let Some(ref mut s) = session {
        service::session::populate_missing_cashier_name(&state.db, s).await;
    }

    Ok(Json(ApiResponse::success(
        session,
        "Active register session retrieved successfully",
    )))
}
