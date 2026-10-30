use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::shift_totals;
use crate::modules::calc::shift_totals::{
    CalcShiftTotalsReply, CalcShiftTotalsRequest, ShiftFigures,
};
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/shift-totals", tag = modules::CALC, request_body = CalcShiftTotalsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "How many of the listed shifts are running, their cash sales and the closed ones' net difference", body = ApiResponse<CalcShiftTotalsReply>),
        (status = 400, description = "Too many shifts", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "A shift was not found", body = ErrorResponse),
    )
)]
/// Needs `register_sessions:view`, like the shift list it totals. Reads the shifts by key.
pub(super) async fn calc_shift_totals(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcShiftTotalsRequest>,
) -> AppResult<Json<ApiResponse<CalcShiftTotalsReply>>> {
    use crate::core::constants::codes;
    use crate::modules::sessions;

    user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    shift_totals::check(&body)?;
    let mut shifts = Vec::with_capacity(body.session_keys.len());
    for key in &body.session_keys {
        let session = sessions::repository::sessions::find_session_by_key(&state.db, key)
            .await?
            .ok_or_else(|| {
                crate::core::error::AppError::not_found_with_code(
                    "Register session not found",
                    codes::SESSION_NOT_FOUND,
                )
            })?;
        shifts.push(ShiftFigures {
            status: session.status,
            cash_sales_cents: session.total_cash_sales_cents,
            cash_variance_cents: session.cash_variance_cents,
        });
    }
    Ok(Json(ApiResponse::success(
        shift_totals::compute(&shifts),
        "Calculated",
    )))
}
