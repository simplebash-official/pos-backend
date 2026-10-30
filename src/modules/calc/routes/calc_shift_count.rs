use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::modules::calc::cash::{CalcShiftCountReply, CalcShiftCountRequest};

use crate::modules::calc::cash;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/shift-count", tag = modules::CALC, request_body = CalcShiftCountRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the drawer should hold, the difference, and whether a reason or PIN is needed", body = ApiResponse<CalcShiftCountReply>),
        (status = 400, description = "Both or neither of shift and terminal given, or an amount is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Shift not found", body = ErrorResponse),
    )
)]
/// Same access as opening or closing a shift (`register_sessions:manage`). Reads the shift or drawer.
pub(super) async fn calc_shift_count(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcShiftCountRequest>,
) -> AppResult<Json<ApiResponse<CalcShiftCountReply>>> {
    use crate::core::constants::codes;
    use crate::modules::sessions;

    user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let (expected, opening_float) = match (&body.session_key, &body.terminal_id) {
        (Some(key), None) => {
            let session = sessions::repository::sessions::find_session_by_key(&state.db, key)
                .await?
                .ok_or_else(|| {
                    crate::core::error::AppError::not_found_with_code(
                        "Register session not found",
                        codes::SESSION_NOT_FOUND,
                    )
                })?;
            (
                session.calculate_expected_cash(),
                Some(session.opening_float_cents),
            )
        }
        (None, Some(terminal_id)) => {
            let drawer =
                sessions::repository::drawer::get_drawer_state(&state.db, terminal_id).await?;
            (
                sessions::service::expected_opening_float(drawer.as_ref(), body.counted_cents),
                None,
            )
        }
        _ => {
            return Err(crate::core::error::AppError::validation(
                "Give either the shift being closed or the terminal being opened",
            ));
        }
    };
    let reply = cash::compute_shift_count(&body, expected, opening_float)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
