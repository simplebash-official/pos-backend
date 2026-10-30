use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::receipt::{CalcReceiptReply, CalcReceiptRequest};

use crate::modules::calc::receipt;
use axum::Json;

#[utoipa::path(post, path = "/receipt", tag = modules::CALC, request_body = CalcReceiptRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every figure for the form as typed so far, and what still stops Save", body = ApiResponse<CalcReceiptReply>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 400, description = "An amount or count is out of range, or there are too many lines", body = ErrorResponse),
    )
)]
/// Same access as recording the receipt, so cost figures stay admin-only.
pub(super) async fn calc_receipt(
    _admin: AdminUser,
    Json(body): Json<CalcReceiptRequest>,
) -> AppResult<Json<ApiResponse<CalcReceiptReply>>> {
    let reply = receipt::compute(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
