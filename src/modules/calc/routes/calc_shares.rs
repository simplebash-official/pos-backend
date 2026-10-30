use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::shares::{CalcSharesReply, CalcSharesRequest};

use crate::modules::calc::shares;
use axum::Json;

#[utoipa::path(post, path = "/shares", tag = modules::CALC, request_body = CalcSharesRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Each part's share of its whole, for progress and split bars", body = ApiResponse<CalcSharesReply>),
        (status = 400, description = "An amount is out of range, or there are too many parts", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
/// Any signed-in user: it only turns figures the screen already shows into bar lengths.
pub(super) async fn calc_shares(
    _user: CurrentUser,
    Json(body): Json<CalcSharesRequest>,
) -> AppResult<Json<ApiResponse<CalcSharesReply>>> {
    let reply = shares::compute(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
