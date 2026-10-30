use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::modules::calc::cash::{CalcCashCountReply, CalcCashCountRequest};

use crate::modules::calc::cash;
use axum::Json;

#[utoipa::path(post, path = "/cash-count", tag = modules::CALC, request_body = CalcCashCountRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every denomination with its subtotal, and the total", body = ApiResponse<CalcCashCountReply>),
        (status = 400, description = "Not a note or coin, or a count is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `register_sessions:view`, like the screens that count cash or show a saved count.
pub(super) async fn calc_cash_count(
    user: CurrentUser,
    Json(body): Json<CalcCashCountRequest>,
) -> AppResult<Json<ApiResponse<CalcCashCountReply>>> {
    user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let reply = cash::compute_count(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
